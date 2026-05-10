import { describe, expect, it } from 'vitest';
import { parsePolicyMeta } from '@lib/policy-meta';

describe('parsePolicyMeta', () => {
  it('parses a single-rule entry with @id, @severity, @reason', () => {
    const text = `@id("user/no-zero-min-output")
@severity("deny")
@reason("Min output of 0 disables slippage protection")
forbid (
  principal is Wallet,
  action == Action::"dex",
  resource is Protocol
)
when {
  context has minOutputUsd && context.minOutputUsd == 0
};
`;
    const meta = parsePolicyMeta(text);
    expect(meta.rules).toEqual([
      { severity: 'deny', reason: 'Min output of 0 disables slippage protection' },
    ]);
    expect(meta.dominantSeverity).toBe('deny');
  });

  it('parses an entry with multiple forbid clauses, each with its own annotations', () => {
    const text = `@id("a/x")
@severity("warn")
@reason("warn case")
forbid (principal, action, resource) when { 1 == 1 };
@id("a/x")
@severity("deny")
@reason("deny case")
forbid (principal, action, resource) when { 1 == 2 };
`;
    const meta = parsePolicyMeta(text);
    expect(meta.rules).toEqual([
      { severity: 'warn', reason: 'warn case' },
      { severity: 'deny', reason: 'deny case' },
    ]);
    expect(meta.dominantSeverity).toBe('deny');
  });

  it('falls back to unknown severity and a default reason when annotations are missing', () => {
    const text = `forbid (principal, action, resource);`;
    const meta = parsePolicyMeta(text);
    expect(meta.rules).toEqual([
      { severity: 'unknown', reason: '(no reason annotation)' },
    ]);
    expect(meta.dominantSeverity).toBe('unknown');
  });

  it('promotes deny over warn over unknown for dominantSeverity', () => {
    const text = `@severity("warn") @reason("w") forbid (principal, action, resource);
@severity("unknown") @reason("u") forbid (principal, action, resource);`;
    expect(parsePolicyMeta(text).dominantSeverity).toBe('warn');
  });

  it('preserves clauses when @reason contains a literal "(" inside a string', () => {
    const text = `@severity("warn")
@reason("careful with (parens)")
forbid (principal, action, resource);
@severity("deny")
@reason("second")
forbid (principal, action, resource);`;
    const meta = parsePolicyMeta(text);
    expect(meta.rules).toEqual([
      { severity: 'warn', reason: 'careful with (parens)' },
      { severity: 'deny', reason: 'second' },
    ]);
  });

  it('preserves clauses when @reason contains a literal ";" inside a string', () => {
    const text = `@severity("warn")
@reason("a;b")
forbid (principal, action, resource);
@severity("deny")
@reason("c")
forbid (principal, action, resource);`;
    const meta = parsePolicyMeta(text);
    expect(meta.rules).toEqual([
      { severity: 'warn', reason: 'a;b' },
      { severity: 'deny', reason: 'c' },
    ]);
  });

  it('decodes JSON-style escapes in @reason and passes unknown escapes through', () => {
    const text = `@severity("warn") @reason("a\\nb\\\\c\\"d\\u0041") forbid (principal, action, resource);
@severity("warn") @reason("\\q passthrough") forbid (principal, action, resource);`;
    const meta = parsePolicyMeta(text);
    expect(meta.rules[0].reason).toBe('a\nb\\c"dA');
    expect(meta.rules[1].reason).toBe('\\q passthrough');
  });

  it('decodes \\r, \\t, and astral \\u{...} codepoints; passes through out-of-range codepoints', () => {
    const text = `@severity("warn") @reason("a\\rb\\tc\\u{1F600}") forbid (principal, action, resource);
@severity("warn") @reason("oob\\u{110000}") forbid (principal, action, resource);`;
    const meta = parsePolicyMeta(text);
    expect(meta.rules[0].reason).toBe('a\rb\tc\u{1F600}');
    expect(meta.rules[1].reason).toBe('oob\\u{110000}');
  });

  it('keeps backslash-quote inside a string from prematurely closing it', () => {
    const text = `@severity("warn") @reason("close\\";then") forbid (principal, action, resource);
@severity("deny") @reason("next") forbid (principal, action, resource);`;
    const meta = parsePolicyMeta(text);
    expect(meta.rules).toEqual([
      { severity: 'warn', reason: 'close";then' },
      { severity: 'deny', reason: 'next' },
    ]);
  });
});
