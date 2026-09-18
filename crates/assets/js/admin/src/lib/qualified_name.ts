import type { QualifiedName } from "@bindings/QualifiedName";

/// Parse a qualified schema name like: `[db].name`.
export function parseQualifiedName(input: string): QualifiedName {
  return new NameParser(input).parseQualified();
}

/// 1:1 adaptation of Rust parser in trailbase-schema.
class NameParser {
  input: string;
  pos: number;

  constructor(input: string) {
    this.input = input.trim();
    this.pos = -1;
  }

  private peek(): string | undefined {
    return this.input.at(this.pos + 1);
  }

  private next(): string | undefined {
    return this.input.at(++this.pos);
  }

  public parseQualified(): QualifiedName {
    const first = this.parseName();

    let database_schema: string | null = null;
    let name: string;

    const nextChar = this.next();
    if (nextChar === ".") {
      database_schema = first;
      name = this.parseName();
    } else {
      name = first;
    }

    // Check that we consumed the entire input.
    const trailing = this.next();
    if (trailing !== undefined) {
      throw `unparsed trailing input '${trailing}': ${this.pos} ${this.input}`;
    }

    return { database_schema, name };
  }

  private parseName(): string {
    // If there's escaping, expect the respective terminator.
    let terminator: string | null;

    const first = this.peek();
    switch (first) {
      case '"':
      case "`":
      case "'":
        terminator = first;
        break;
      case "[":
        terminator = "]";
        break;
      case undefined:
        throw `expected identifier, got undefined`;
      default:
        if (isAlphabetic(first) || first === "_") {
          // Valid unescaped name identifier character.
          terminator = null;
        } else {
          throw `expected identifier, got '${first}'`;
        }
        break;
    }

    // Consume the opening delimiter, if any.
    if (terminator !== null) {
      this.next();
    }

    let out = "";
    if (terminator === null) {
      // Unescaped case, scanning until end-of-input or first non-identifier character.
      while (true) {
        const c = this.peek();
        if (c !== undefined && c !== ";" && c !== ".") {
          out += c;
          // Only advance iterator if character was part of the name.
          this.next();
        } else {
          // Stop consuming at first non-valid identifier or end-of-input
          return out;
        }
      }
    } else {
      // Escaped case, scanning for terminator.
      while (true) {
        const c = this.next();
        if (c === undefined) {
          // We reached the end-of-input w/o finding the terminator.
          throw "unterminated quoted identifier";
        } else if (c !== terminator) {
          // Continue until we reach terminator.
          out += c;
        } else {
          // We reached the terminator.
          return out;
        }
      }
    }
  }
}

function isAlphabetic(c: string): boolean {
  return /\p{L}/u.test(c);
}
