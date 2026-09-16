import { once } from 'node:events'
import {
  createServer,
  type ServerHttp2Session,
  type ServerHttp2Stream,
} from 'node:http2'
import { isAbsolute, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import protobuf from 'protobufjs'

// Decode the repository schema so the binding test checks the actual wire fields.
export async function notificationBackend() {
  const protoRoot = fileURLToPath(new URL('../../../proto/', import.meta.url))
  const root = new protobuf.Root()
  root.resolvePath = (_, target) =>
    isAbsolute(target) ? target : resolve(protoRoot, target)
  await root.load('backend/v1/backend.proto')
  const register = root.lookupType('xmtp.backend.v1.RegisterRequest')
  const recipient = root.lookupType('xmtp.backend.v1.RecipientState')
  const inboxRequest = root.lookupType('xmtp.backend.v1.GetInboxIdsRequest')
  const inboxResponse = root.lookupType('xmtp.backend.v1.GetInboxIdsResponse')
  const queryResponse = root.lookupType('xmtp.backend.v1.QueryResponse')
  const configurationResponse = root.lookupType(
    'xmtp.backend.v1.GetConfigurationResponse'
  )
  const registrations: Array<{
    apns?: { token: string }
    fcm?: { token: string }
    http?: { url: string; signingKey: number[] }
  }> = []
  const server = createServer()
  const sessions = new Set<ServerHttp2Session>()
  server.on('session', (session) => {
    sessions.add(session)
    session.on('close', () => sessions.delete(session))
  })
  server.on('stream', (stream: ServerHttp2Stream, headers) => {
    const chunks: Buffer[] = []
    stream.on('data', (chunk: Buffer) => chunks.push(chunk))
    stream.on('end', () => {
      const frame = Buffer.concat(chunks)
      let payload = Buffer.alloc(0)
      if (String(headers[':path']).endsWith('/Query')) {
        payload = Buffer.from(
          queryResponse
            .encode(queryResponse.fromObject({ continuation: {} }))
            .finish()
        )
      }
      // Spec 006 CFG-040: `build` resolves the deployment's configuration
      // before any identity work, so this fake has to publish an identifier.
      // Every other field is left at zero, which CFG-031 reads as "not
      // provided" and fills from the client's compiled defaults.
      if (String(headers[':path']).endsWith('/GetConfiguration')) {
        payload = Buffer.from(
          configurationResponse
            .encode(
              configurationResponse.fromObject({
                identifier: 'test.notification.backend',
              })
            )
            .finish()
        )
      }
      if (String(headers[':path']).endsWith('/GetInboxIds')) {
        const request = inboxRequest.toObject(
          inboxRequest.decode(frame.subarray(5))
        )
        payload = Buffer.from(
          inboxResponse
            .encode(inboxResponse.fromObject({ responses: request.requests }))
            .finish()
        )
      }
      if (
        headers[':path'] === '/xmtp.backend.v1.NotificationService/Register'
      ) {
        const request = register.toObject(register.decode(frame.subarray(5)), {
          bytes: Array,
        }) as (typeof registrations)[number]
        registrations.push(request)
        payload = Buffer.from(
          recipient
            .encode(
              recipient.fromObject({
                channel: request.http ? 3 : request.fcm ? 2 : 1,
                expiresAtNs: (
                  BigInt(Date.now()) * 1_000_000n +
                  86_400_000_000_000n
                ).toString(),
              })
            )
            .finish()
        )
      }
      // Other calls see an empty backend: no inbox, messages, or subscriptions.
      const response = Buffer.alloc(5 + payload.length)
      response.writeUInt32BE(payload.length, 1)
      payload.copy(response, 5)
      stream.respond(
        { ':status': 200, 'content-type': 'application/grpc' },
        { waitForTrailers: true }
      )
      stream.on('wantTrailers', () =>
        stream.sendTrailers({ 'grpc-status': '0' })
      )
      stream.end(response)
    })
  })
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const address = server.address()
  if (!address || typeof address === 'string')
    throw new Error('missing listener address')
  return {
    url: `http://127.0.0.1:${address.port}`,
    registrations,
    close: async () => {
      for (const session of sessions) session.destroy()
      await new Promise<void>((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve()))
      )
    },
  }
}
