import { randomBytes } from 'node:crypto'
import { createServer, type Server } from 'node:http'
import { getKnowledgeService } from '../service'

type Gateway = { endpoint: string; token: string; server: Server }

let gatewayPromise: Promise<Gateway> | null = null

function readBody(request: import('node:http').IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    let body = ''
    request.setEncoding('utf8')
    request.on('data', (chunk: string) => {
      body += chunk
      if (body.length > 64 * 1024) request.destroy(new Error('Request is too large'))
    })
    request.once('end', () => resolve(body))
    request.once('error', reject)
  })
}

export function ensureKnowledgeAgentGateway(): Promise<{ endpoint: string; token: string }> {
  if (!gatewayPromise) {
    gatewayPromise = new Promise<Gateway>((resolve, reject) => {
      const token = randomBytes(32).toString('hex')
      const server = createServer(async (request, response) => {
        response.setHeader('Content-Type', 'application/json; charset=utf-8')
        if (request.method !== 'POST' || request.headers.authorization !== `Bearer ${token}`) {
          response.statusCode = 404
          response.end(JSON.stringify({ error: 'Not found' }))
          return
        }
        try {
          const parsed = JSON.parse(await readBody(request)) as {
            query?: unknown
            limit?: unknown
            resultId?: unknown
            maxChars?: unknown
          }
          if (request.url === '/search') {
            const query = typeof parsed.query === 'string' ? parsed.query.trim() : ''
            const limit = typeof parsed.limit === 'number'
              ? Math.max(1, Math.min(20, Math.round(parsed.limit)))
              : 8
            if (!query) throw new Error('query is required')
            const hits = getKnowledgeService().search(query, limit)
            response.end(JSON.stringify({ hits }))
            return
          }
          if (request.url === '/read') {
            const resultId = typeof parsed.resultId === 'string' ? parsed.resultId.trim() : ''
            const maxChars = typeof parsed.maxChars === 'number'
              ? Math.max(500, Math.min(50_000, Math.round(parsed.maxChars)))
              : 12_000
            if (!resultId) throw new Error('resultId is required')
            const result = getKnowledgeService().read(resultId, maxChars)
            if (!result) {
              response.statusCode = 404
              response.end(JSON.stringify({ error: 'Knowledge result was not found' }))
              return
            }
            response.end(JSON.stringify({ result }))
            return
          }
          response.statusCode = 404
          response.end(JSON.stringify({ error: 'Not found' }))
        } catch (error) {
          response.statusCode = 400
          response.end(JSON.stringify({ error: error instanceof Error ? error.message : String(error) }))
        }
      })
      server.once('error', reject)
      server.listen(0, '127.0.0.1', () => {
        const address = server.address()
        if (!address || typeof address === 'string') {
          reject(new Error('Failed to start the local knowledge bridge'))
          return
        }
        resolve({ endpoint: `http://127.0.0.1:${address.port}/search`, token, server })
      })
    })
  }
  return gatewayPromise.then(({ endpoint, token }) => ({ endpoint, token }))
}

export async function stopKnowledgeAgentGateway(): Promise<void> {
  if (!gatewayPromise) return
  const gateway = await gatewayPromise.catch(() => null)
  gatewayPromise = null
  if (!gateway) return
  await new Promise<void>((resolve) => gateway.server.close(() => resolve()))
}
