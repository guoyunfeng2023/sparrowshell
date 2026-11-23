/**
 * Best-effort local line editor for manual terminal history. Shell completion,
 * recalled history and other uncertain edits are deliberately not persisted.
 */
export class CommandLineRecorder {
  private characters: string[] = [];
  private cursor = 0;
  private escape: 'none' | 'escape' | 'csi' = 'none';
  private sequence = '';
  private uncertain = false;

  snapshot(): { text: string; cursor: number; confident: boolean } {
    return {
      text: this.characters.join(''),
      cursor: this.cursor,
      confident: !this.uncertain && this.escape === 'none',
    };
  }

  clear(): void {
    this.reset();
  }

  accept(data: string): string[] {
    const completed: string[] = [];
    for (const character of data) {
      if (this.escape === 'escape') {
        if (character === '[') {
          this.escape = 'csi';
          this.sequence = '';
        } else {
          this.escape = 'none';
          this.uncertain = true;
        }
        continue;
      }
      if (this.escape === 'csi') {
        this.sequence += character;
        if (character >= '@' && character <= '~') {
          this.handleCsi(this.sequence);
          this.escape = 'none';
          this.sequence = '';
        }
        continue;
      }
      if (character === '\u001b') {
        this.escape = 'escape';
        continue;
      }
      if (character === '\r' || character === '\n') {
        const command = this.characters.join('').trim();
        if (command && !this.uncertain) completed.push(command);
        this.reset();
        continue;
      }
      this.handleInput(character);
    }
    return completed;
  }

  private handleInput(character: string) {
    switch (character) {
      case '\u0003': // Ctrl+C
        this.reset();
        return;
      case '\u0001': // Ctrl+A
        this.cursor = 0;
        return;
      case '\u0005': // Ctrl+E
        this.cursor = this.characters.length;
        return;
      case '\u000b': // Ctrl+K
        this.characters.splice(this.cursor);
        return;
      case '\u0015': // Ctrl+U
        this.characters.splice(0, this.cursor);
        this.cursor = 0;
        return;
      case '\u0017': // Ctrl+W
        this.deleteWord();
        return;
      case '\u007f':
      case '\b':
        if (this.cursor > 0) {
          this.characters.splice(this.cursor - 1, 1);
          this.cursor -= 1;
        }
        return;
      case '\u000c': // Ctrl+L only changes display.
      case '\u0004': // EOF has no command text when the line is empty.
        return;
      case '\t':
        this.uncertain = true;
        return;
      default:
        if (character < ' ' || character === '\u007f') {
          this.uncertain = true;
          return;
        }
        this.characters.splice(this.cursor, 0, character);
        this.cursor += 1;
    }
  }

  private handleCsi(sequence: string) {
    const final = sequence.at(-1);
    const body = sequence.slice(0, -1);
    if (body === '200' || body === '201') return; // Bracketed paste markers.
    if (final === 'A' || final === 'B') {
      this.uncertain = true; // Shell-recalled history is not visible locally.
      return;
    }
    if (body && body !== '1' && body !== '3') {
      this.uncertain = true;
      return;
    }
    if (final === 'C') this.cursor = Math.min(this.characters.length, this.cursor + 1);
    else if (final === 'D') this.cursor = Math.max(0, this.cursor - 1);
    else if (final === 'H') this.cursor = 0;
    else if (final === 'F') this.cursor = this.characters.length;
    else if (final === '~' && body === '3') this.characters.splice(this.cursor, 1);
    else if (final !== '~') this.uncertain = true;
  }

  private deleteWord() {
    let start = this.cursor;
    while (start > 0 && /\s/.test(this.characters[start - 1])) start -= 1;
    while (start > 0 && !/\s/.test(this.characters[start - 1])) start -= 1;
    this.characters.splice(start, this.cursor - start);
    this.cursor = start;
  }

  private reset() {
    this.characters = [];
    this.cursor = 0;
    this.uncertain = false;
    this.escape = 'none';
    this.sequence = '';
  }
}
