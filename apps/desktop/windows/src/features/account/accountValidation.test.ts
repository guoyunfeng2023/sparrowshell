import { describe, expect, it } from 'vitest';
import { isValidAccountPassword, isValidNickname, isValidVerificationCode } from './accountValidation';

describe('account validation', () => {
  it('requires at least eight characters with letters and digits or symbols', () => {
    expect(isValidAccountPassword('abcdefg1')).toBe(true);
    expect(isValidAccountPassword('abcdefg!')).toBe(true);
    expect(isValidAccountPassword('abc123!@#')).toBe(true);
    for (const password of ['abcdefgh', '12345678', '1234!@#$', 'abc123', 'abcd 123', '密码abcd123']) {
      expect(isValidAccountPassword(password)).toBe(false);
    }
  });

  it('accepts only six digit verification codes', () => {
    expect(isValidVerificationCode('123456')).toBe(true);
    expect(isValidVerificationCode('12345')).toBe(false);
    expect(isValidVerificationCode('abcdef')).toBe(false);
  });

  it('limits display names to thirty nonblank characters', () => {
    expect(isValidNickname(' Sparrow ')).toBe(true);
    expect(isValidNickname('   ')).toBe(false);
    expect(isValidNickname('a'.repeat(30))).toBe(true);
    expect(isValidNickname('a'.repeat(31))).toBe(false);
  });
});
