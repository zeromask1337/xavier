export class UsageError extends Error {
  readonly exitCode = 2;

  constructor(message: string) {
    super(message);
    this.name = "UsageError";
  }
}

export class XavierError extends Error {
  readonly exitCode: number;
  readonly hint?: string;

  constructor(message: string, opts: { hint?: string; exitCode?: number } = {}) {
    super(message);
    this.name = "XavierError";
    this.hint = opts.hint;
    this.exitCode = opts.exitCode ?? 1;
  }
}
