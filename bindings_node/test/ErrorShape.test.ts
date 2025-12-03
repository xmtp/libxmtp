import { describe, expect, it } from 'vitest'

import {
  throwGroupNotFoundError,
  throwIdentityTooManyInstallationsError,
} from '../dist/index'

describe('structured errors', () => {
  it('exposes code and details for GroupError::NotFound (node binding)', () => {
    let err: any
    try {
      throwGroupNotFoundError()
    } catch (e) {
      err = e
    }

    expect(err?.code).toBe('GroupError::NotFound')
    expect(err?.details?.entity).toBe('test-entity')
  })

  it('exposes code and details for IdentityError::TooManyInstallations (node binding)', () => {
    let err: any
    try {
      throwIdentityTooManyInstallationsError()
    } catch (e) {
      err = e
    }

    expect(err?.code).toBe('IdentityError::TooManyInstallations')
    expect(err?.details?.inboxId).toBe('test-inbox')
    expect(err?.details?.count).toBe(2)
    expect(err?.details?.max).toBe(1)
  })
})
