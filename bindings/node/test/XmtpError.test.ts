import { describe, expect, it } from 'vitest'
import { parseXmtpError, XmtpError } from '../xmtp_error'

describe('XmtpError', () => {
  describe('XmtpError class', () => {
    it('should be an instance of Error', () => {
      const error = new XmtpError('test message', 'TestError::Code')
      expect(error instanceof Error).toBe(true)
    })

    it('should be an instance of XmtpError', () => {
      const error = new XmtpError('test message', 'TestError::Code')
      expect(error instanceof XmtpError).toBe(true)
    })

    it('should have name set to XmtpError', () => {
      const error = new XmtpError('test message', 'TestError::Code')
      expect(error.name).toBe('XmtpError')
    })

    it('should have correct code and message', () => {
      const error = new XmtpError('test message', 'TestError::Code')
      expect(error.code).toBe('TestError::Code')
      expect(error.message).toBe('test message')
    })
  })

  describe('parseXmtpError', () => {
    it('should parse error with code prefix', () => {
      const error = new Error('[GroupError::NotFound] Group not found')
      const parsed = parseXmtpError(error)

      expect(parsed.code).toBe('GroupError::NotFound')
      expect(parsed.message).toBe('[GroupError::NotFound] Group not found')
    })

    it('should return XmtpError instance', () => {
      const error = new Error('[GroupError::NotFound] Group not found')
      const parsed = parseXmtpError(error)

      expect(parsed instanceof XmtpError).toBe(true)
      expect(parsed instanceof Error).toBe(true)
    })

    it('should parse error with nested code', () => {
      const error = new Error('[ClientError::GroupError] Something went wrong')
      const parsed = parseXmtpError(error)

      expect(parsed.code).toBe('ClientError::GroupError')
    })

    it('should return Unknown for errors without code prefix', () => {
      const error = new Error('Some regular error')
      const parsed = parseXmtpError(error)

      expect(parsed.code).toBe('Unknown')
      expect(parsed.message).toBe('Some regular error')
      expect(parsed instanceof XmtpError).toBe(true)
    })

    it('should handle non-Error values', () => {
      const parsed = parseXmtpError('string error')

      expect(parsed.code).toBe('Unknown')
      expect(parsed.message).toBe('string error')
      expect(parsed instanceof XmtpError).toBe(true)
    })

    it('should handle null/undefined', () => {
      expect(parseXmtpError(null).code).toBe('Unknown')
      expect(parseXmtpError(undefined).code).toBe('Unknown')
      expect(parseXmtpError(null) instanceof XmtpError).toBe(true)
    })

    it('should preserve stack trace', () => {
      const error = new Error('[TestError::Stack] Test')
      const parsed = parseXmtpError(error)

      expect(parsed.stack).toBe(error.stack)
    })
  })

  describe('usage patterns', () => {
    it('should support instanceof checks', () => {
      const error = new Error('[GroupError::NotFound] Group was deleted')
      const parsed = parseXmtpError(error)

      // instanceof check works
      if (parsed instanceof XmtpError) {
        expect(parsed.code).toBe('GroupError::NotFound')
      } else {
        throw new Error('Should be instanceof XmtpError')
      }
    })

    it('should support parse and switch pattern', () => {
      const error = new Error('[GroupError::NotFound] Group was deleted')
      const parsed = parseXmtpError(error)

      let result: string
      switch (parsed.code) {
        case 'GroupError::NotFound':
          result = 'not found'
          break
        case 'GroupError::PermissionDenied':
          result = 'permission denied'
          break
        default:
          result = 'unknown'
      }

      expect(result).toBe('not found')
    })

    it('should support category matching with startsWith', () => {
      const groupError = new Error('[GroupError::NotFound] Not found')
      const clientError = new Error(
        '[ClientError::NotRegistered] Not registered'
      )

      const parsedGroup = parseXmtpError(groupError)
      const parsedClient = parseXmtpError(clientError)

      expect(parsedGroup.code.startsWith('GroupError::')).toBe(true)
      expect(parsedGroup.code.startsWith('ClientError::')).toBe(false)

      expect(parsedClient.code.startsWith('ClientError::')).toBe(true)
      expect(parsedClient.code.startsWith('GroupError::')).toBe(false)
    })

    it('should support try/catch with instanceof', () => {
      function simulateXmtpError(): never {
        const error = new Error('[GroupError::NotFound] Group not found')
        throw parseXmtpError(error)
      }

      try {
        simulateXmtpError()
      } catch (e) {
        if (e instanceof XmtpError) {
          expect(e.code).toBe('GroupError::NotFound')
          expect(e.name).toBe('XmtpError')
        } else {
          throw new Error('Should be caught as XmtpError')
        }
      }
    })
  })
})
