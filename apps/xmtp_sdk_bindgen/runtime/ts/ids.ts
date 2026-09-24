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
  static fromString(value: string): InboxID {
    if (value.length === 0) throw new Error("inbox ID is empty");
    return new InboxID(value);
  }
}

export class InstallationID extends StringID {
  static fromString(value: string): InstallationID {
    checkHex(value, 32);
    return new InstallationID(value);
  }
}

export class ConversationID extends StringID {
  static fromString(value: string): ConversationID {
    checkHex(value, 16);
    return new ConversationID(value);
  }
}

export class MessageID extends StringID {
  static fromString(value: string): MessageID {
    checkHex(value, 32);
    return new MessageID(value);
  }
}

export class Timestamp {
  constructor(readonly ns: bigint) {}

  get date(): Date {
    return new Date(Number(this.ns / 1_000_000n));
  }
}
