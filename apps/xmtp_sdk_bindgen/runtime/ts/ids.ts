const hex = /^[0-9a-f]+$/;

abstract class StringID {
  protected constructor(readonly value: string) {}

  toString(): string {
    return this.value;
  }
}

function checkHex(value: string, bytes: number): void {
  if (value.length !== bytes * 2 || !hex.test(value)) {
    throw new Error("invalid lowercase hex ID");
  }
}

export class InboxID extends StringID {
  /** @internal Values lifted from Rust are already valid. */
  static fromRust(value: string): InboxID {
    return new InboxID(value);
  }

  static fromString(value: string): InboxID {
    if (value.length === 0) throw new Error("inbox ID is empty");
    return new InboxID(value);
  }
}

export class InstallationID extends StringID {
  /** @internal Values lifted from Rust are already valid. */
  static fromRust(value: string): InstallationID {
    return new InstallationID(value);
  }

  static fromString(value: string): InstallationID {
    checkHex(value, 32);
    return new InstallationID(value);
  }
}

export class ConversationID extends StringID {
  /** @internal Values lifted from Rust are already valid. */
  static fromRust(value: string): ConversationID {
    return new ConversationID(value);
  }

  static fromString(value: string): ConversationID {
    checkHex(value, 16);
    return new ConversationID(value);
  }
}

export class MessageID extends StringID {
  /** @internal Values lifted from Rust are already valid. */
  static fromRust(value: string): MessageID {
    return new MessageID(value);
  }

  static fromString(value: string): MessageID {
    checkHex(value, 32);
    return new MessageID(value);
  }
}

export class Timestamp {
  constructor(readonly ns: bigint) {}

  get date(): Date {
    const milliseconds = this.ns / 1_000_000n;
    const remainder = this.ns % 1_000_000n;
    return new Date(Number(milliseconds - (remainder < 0n ? 1n : 0n)));
  }
}
