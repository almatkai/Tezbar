import { app } from '@tezbar/desktop-runtime'
import Database from 'better-sqlite3'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'

import type { AgentTimelineItem, Stage } from '../../shared/agent'
import type {
  ChatAttachment,
  ChatResponseMeta,
  ChatRole,
  ChatSession,
  ChatSessionSummary,
  ChatTurn,
} from '../../shared/chat'

type SessionRow = {
  id: string
  title: string
  created_at: number
  updated_at: number
  working_directory: string | null
}

type TurnRow = {
  id: string
  session_id: string
  role: string
  text: string
  response_meta_json: string | null
  stages_json: string | null
  timeline_json: string | null
  attachments_json: string | null
  error: string | null
  created_at: number
}

function safeParseResponseMeta(raw: string | null): ChatResponseMeta | undefined {
  if (!raw) return undefined
  try {
    const parsed = JSON.parse(raw) as unknown
    if (!parsed || typeof parsed !== 'object') return undefined
    const meta = parsed as Partial<ChatResponseMeta>
    if (
      typeof meta.provider !== 'string' ||
      typeof meta.providerTitle !== 'string' ||
      typeof meta.model !== 'string'
    ) {
      return undefined
    }
    return {
      provider: meta.provider.slice(0, 80),
      providerTitle: meta.providerTitle.slice(0, 120),
      model: meta.model.slice(0, 160),
      tokenCount:
        typeof meta.tokenCount === 'number' && Number.isFinite(meta.tokenCount)
          ? Math.max(0, Math.round(meta.tokenCount))
          : undefined,
    }
  } catch {
    return undefined
  }
}

function safeParseStages(raw: string | null): Stage[] | undefined {
  if (!raw) return undefined
  try {
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return undefined
    const stages = parsed.filter((item): item is Stage => {
      if (!item || typeof item !== 'object') return false
      const stage = item as Partial<Stage>
      return (
        typeof stage.index === 'number' &&
        typeof stage.label === 'string' &&
        (stage.status === 'running' || stage.status === 'done' || stage.status === 'failed')
      )
    })
    return stages.length > 0 ? stages : undefined
  } catch {
    return undefined
  }
}

function safeParseTimeline(raw: string | null): AgentTimelineItem[] | undefined {
  if (!raw) return undefined
  try {
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return undefined
    const timeline = parsed.filter((item): item is AgentTimelineItem => {
      if (!item || typeof item !== 'object') return false
      const entry = item as { type?: unknown; text?: unknown; stage?: unknown }
      if (entry.type === 'text') return typeof entry.text === 'string' && entry.text.length > 0
      if (entry.type !== 'stage' || !entry.stage || typeof entry.stage !== 'object') return false
      const stage = entry.stage as Partial<Stage>
      return (
        typeof stage.index === 'number' &&
        typeof stage.label === 'string' &&
        (stage.status === 'running' || stage.status === 'done' || stage.status === 'failed')
      )
    })
    return timeline.length > 0 ? timeline : undefined
  } catch {
    return undefined
  }
}

function safeParseAttachments(raw: string | null): ChatAttachment[] | undefined {
  if (!raw) return undefined
  try {
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return undefined
    const attachments = parsed.filter((item): item is ChatAttachment => {
      if (!item || typeof item !== 'object') return false
      const attachment = item as Partial<ChatAttachment>
      return (
        attachment.kind === 'image' &&
        typeof attachment.name === 'string' &&
        (attachment.mimeType === 'image/png' ||
          attachment.mimeType === 'image/jpeg' ||
          attachment.mimeType === 'image/webp')
      )
    })
    return attachments.length > 0 ? attachments : undefined
  } catch {
    return undefined
  }
}

function dbPath(): string {
  const dir = join(app.getPath('userData'), 'chat')
  mkdirSync(dir, { recursive: true })
  return join(dir, 'sessions.sqlite3')
}

class ChatSessionDatabase {
  private _db: InstanceType<typeof Database> | null = null
  private _initPromise: Promise<void> | null = null

  private get db(): InstanceType<typeof Database> {
    if (!this._db) {
      throw new Error('Database not initialized - call ensureInitialized() first')
    }
    return this._db
  }

  async ensureInitialized(): Promise<void> {
    if (this._initPromise) return this._initPromise

    this._initPromise = new Promise((resolve) => {
      // Defer database initialization to avoid blocking app startup
      setImmediate(() => {
        this._db = new Database(dbPath())
        this._db.pragma('journal_mode = WAL')
        this._db.pragma('synchronous = NORMAL')
        this._db.pragma('foreign_keys = ON')
        this.bootstrap()
        resolve()
      })
    })

    return this._initPromise
  }

  private bootstrap(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS chat_sessions (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        working_directory TEXT
      );
      CREATE TABLE IF NOT EXISTS chat_turns (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        role TEXT NOT NULL,
        text TEXT NOT NULL,
        response_meta_json TEXT,
        stages_json TEXT,
        timeline_json TEXT,
        attachments_json TEXT,
        error TEXT,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE
      );
      CREATE INDEX IF NOT EXISTS idx_chat_turns_session ON chat_turns(session_id);
      CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated ON chat_sessions(updated_at DESC);
    `)
    try {
      this.db.exec(`ALTER TABLE chat_sessions ADD COLUMN working_directory TEXT;`)
    } catch {
      // Existing and newly-created databases already have the column.
    }
    try {
      this.db.exec(`ALTER TABLE chat_turns ADD COLUMN attachments_json TEXT;`)
    } catch {
      // Existing and newly-created databases already have the column.
    }
    try {
      this.db.exec(`ALTER TABLE chat_turns ADD COLUMN response_meta_json TEXT;`)
    } catch {
      // Existing and newly-created databases already have the column.
    }
    try {
      this.db.exec(`ALTER TABLE chat_turns ADD COLUMN timeline_json TEXT;`)
    } catch {
      // Existing databases are upgraded in place.
    }
  }

  listSessions(limit = 100): ChatSessionSummary[] {
    const rows = this.db
      .prepare(
        `SELECT s.id, s.title, s.created_at, s.updated_at, s.working_directory,
                (SELECT COUNT(*) FROM chat_turns t WHERE t.session_id = s.id) AS turn_count,
                (SELECT t.text FROM chat_turns t
                   WHERE t.session_id = s.id AND t.role = 'user'
                   ORDER BY t.created_at DESC LIMIT 1) AS preview
         FROM chat_sessions s
         ORDER BY s.updated_at DESC
         LIMIT ?`
      )
      .all(limit) as Array<SessionRow & { turn_count: number | bigint; preview: string | null }>
    return rows.map((r) => ({
      id: r.id,
      title: r.title,
      createdAt: r.created_at,
      updatedAt: r.updated_at,
      turnCount: Number(r.turn_count),
      workingDirectory: r.working_directory ?? undefined,
      preview: r.preview ?? '',
    }))
  }

  getSession(id: string): ChatSession | null {
    const sessionRow = this.db
      .prepare(
        `SELECT id, title, created_at, updated_at, working_directory FROM chat_sessions WHERE id = ?`
      )
      .get(id) as SessionRow | undefined
    if (!sessionRow) return null
    const turnRows = this.db
      .prepare(
        `SELECT id, session_id, role, text, response_meta_json, stages_json, timeline_json, attachments_json, error, created_at
         FROM chat_turns WHERE session_id = ? ORDER BY created_at ASC`
      )
      .all(id) as TurnRow[]
    return {
      id: sessionRow.id,
      title: sessionRow.title,
      createdAt: sessionRow.created_at,
      updatedAt: sessionRow.updated_at,
      workingDirectory: sessionRow.working_directory ?? undefined,
      turns: turnRows.map((t) => ({
        id: t.id,
        role: (t.role === 'assistant' ? 'assistant' : 'user') as ChatRole,
        text: t.text,
        responseMeta: safeParseResponseMeta(t.response_meta_json),
        stages: safeParseStages(t.stages_json),
        timeline: safeParseTimeline(t.timeline_json),
        attachments: safeParseAttachments(t.attachments_json),
        error: t.error ?? undefined,
        createdAt: t.created_at,
      })),
    }
  }

  upsertSession(
    session: Pick<ChatSession, 'id' | 'title' | 'createdAt' | 'updatedAt' | 'workingDirectory'>
  ): void {
    this.db
      .prepare(
        `INSERT INTO chat_sessions(id, title, created_at, updated_at, working_directory)
         VALUES(?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           title = excluded.title,
           updated_at = excluded.updated_at,
           working_directory = excluded.working_directory`
      )
      .run(
        session.id,
        session.title,
        session.createdAt,
        session.updatedAt,
        session.workingDirectory ?? null
      )
  }

  appendTurn(sessionId: string, turn: ChatTurn): void {
    this.db
      .prepare(
        `INSERT INTO chat_turns(id, session_id, role, text, response_meta_json, stages_json, timeline_json, attachments_json, error, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           text = excluded.text,
           response_meta_json = excluded.response_meta_json,
           stages_json = excluded.stages_json,
           timeline_json = excluded.timeline_json,
           attachments_json = excluded.attachments_json,
           error = excluded.error`
      )
      .run(
        turn.id,
        sessionId,
        turn.role,
        turn.text,
        turn.responseMeta ? JSON.stringify(turn.responseMeta) : null,
        turn.stages ? JSON.stringify(turn.stages) : null,
        turn.timeline ? JSON.stringify(turn.timeline) : null,
        turn.attachments
          ? JSON.stringify(
              turn.attachments.map((attachment) => {
                const metadata = { ...attachment }
                delete metadata.data
                return metadata
              })
            )
          : null,
        turn.error ?? null,
        turn.createdAt
      )
    this.db
      .prepare(`UPDATE chat_sessions SET updated_at = ? WHERE id = ?`)
      .run(Math.max(turn.createdAt, Date.now()), sessionId)
  }

  updateTitle(sessionId: string, title: string): void {
    this.db
      .prepare(`UPDATE chat_sessions SET title = ?, updated_at = ? WHERE id = ?`)
      .run(title, Date.now(), sessionId)
  }

  deleteTurn(sessionId: string, turnId: string): boolean {
    const info = this.db
      .prepare(`DELETE FROM chat_turns WHERE session_id = ? AND id = ?`)
      .run(sessionId, turnId)
    if (info.changes > 0) {
      this.db
        .prepare(`UPDATE chat_sessions SET updated_at = ? WHERE id = ?`)
        .run(Date.now(), sessionId)
    }
    return info.changes > 0
  }

  deleteSession(id: string): boolean {
    const info = this.db.prepare(`DELETE FROM chat_sessions WHERE id = ?`).run(id)
    return info.changes > 0
  }

  clearAll(): void {
    this.db.exec(`DELETE FROM chat_turns; DELETE FROM chat_sessions;`)
  }
}

let instance: ChatSessionDatabase | null = null

function store(): ChatSessionDatabase {
  if (!instance) instance = new ChatSessionDatabase()
  return instance
}

export async function listChatSessions(limit?: number): Promise<ChatSessionSummary[]> {
  await store().ensureInitialized()
  return store().listSessions(limit)
}

export async function getChatSession(id: string): Promise<ChatSession | null> {
  await store().ensureInitialized()
  return store().getSession(id)
}

export async function upsertChatSession(
  session: Pick<ChatSession, 'id' | 'title' | 'createdAt' | 'updatedAt' | 'workingDirectory'>
): Promise<void> {
  await store().ensureInitialized()
  store().upsertSession(session)
}

export async function appendChatTurn(sessionId: string, turn: ChatTurn): Promise<void> {
  await store().ensureInitialized()
  store().appendTurn(sessionId, turn)
}

export async function updateChatSessionTitle(sessionId: string, title: string): Promise<void> {
  await store().ensureInitialized()
  store().updateTitle(sessionId, title)
}

export async function deleteChatTurn(sessionId: string, turnId: string): Promise<boolean> {
  await store().ensureInitialized()
  return store().deleteTurn(sessionId, turnId)
}

export async function deleteChatSession(id: string): Promise<boolean> {
  await store().ensureInitialized()
  return store().deleteSession(id)
}

export async function clearAllChatSessions(): Promise<void> {
  await store().ensureInitialized()
  store().clearAll()
}
