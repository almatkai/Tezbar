export const TERMINAL_IPC = {
  CREATE: 'terminal:create',
  WRITE: 'terminal:write',
  RESIZE: 'terminal:resize',
  KILL: 'terminal:kill',
  DATA: 'terminal:data',
  EXIT: 'terminal:exit',
  GET_PROMPT_INFO: 'terminal:get-prompt-info',
} as const

export type TerminalCreateRequest = {
  cwd?: string
  initialCommand?: string
  cols: number
  rows: number
}

export type TerminalCreateResult = {
  sessionId: string
  shell: string
  cwd: string
}

export type TerminalDataEvent = {
  sessionId: string
  data: string
}

export type TerminalExitEvent = {
  sessionId: string
  exitCode: number
  signal?: number
}

export type TerminalPromptInfo = {
  user: string
  host: string
  dir: string
}
