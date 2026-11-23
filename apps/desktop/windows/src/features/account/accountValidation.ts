export function isValidAccountPassword(password: string): boolean {
  return /^(?=.*[A-Za-z])(?=.*[^A-Za-z])[\x21-\x7E]{8,256}$/.test(password);
}

export function isValidVerificationCode(code: string): boolean {
  return /^\d{6}$/.test(code);
}

export function isValidNickname(nickname: string): boolean {
  const length = Array.from(nickname.trim()).length;
  return length >= 1 && length <= 30;
}
