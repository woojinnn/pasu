// Migration Rewrite banner tests (Phase 7.5).
//
// The banner reads `listMigrationPending()` on mount and offers a
// "Rewrite" button per id. Clicking Rewrite walks the full migration
// flow:
//
//   1. `rewritePolicyToCustom({id, text, knownFields})` → returns
//      `{rewritten, applied}` (does NOT persist; spec §"Migration").
//   2. If `applied`, `putRaw({id, text: rewritten})` to persist the v1
//      text into managed-policy storage.
//   3. `migrationAck(id)` to pop the id off the pending queue. Splitting
//      the rewrite and ack avoids a window where pending is empty but
//      storage still holds v0 text (handler comment, handlers.ts:196).
//
// If the handler returns `applied: false` (nothing to rewrite — already
// on v1 layout) the SW auto-acks; the banner still refreshes.

import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import type {
  ExtensionClient,
  ManagedPolicy,
  MigrationRewriteResult,
} from "@scopeball/sdk";
import { RewriteBanner } from "./rewrite-banner";
import { TestSdkProvider } from "../testing/test-sdk-provider";

function managed(id: string, text: string): ManagedPolicy {
  return {
    id,
    kind: "raw",
    text,
    updatedAtMs: 1_700_000_000_000,
    schemaVersion: 1,
  };
}

function renderBanner(overrides: Partial<ExtensionClient>) {
  const client = {
    listMigrationPending: vi.fn(async () => ({ ids: [] as string[] })),
    listManaged: vi.fn(async () => [] as ManagedPolicy[]),
    rewritePolicyToCustom: vi.fn(
      async (args: { id: string }): Promise<MigrationRewriteResult> => ({
        id: args.id,
        rewritten: "rewritten-text",
        applied: true,
      }),
    ),
    putRaw: vi.fn(async () => ({
      policy: managed("dashboard::p1", "rewritten-text"),
      catalog: { policies: [], enabled: [] },
    })),
    migrationAck: vi.fn(async (id: string) => ({ id, remaining: [] as string[] })),
    ...overrides,
  } as unknown as ExtensionClient;
  const utils = render(
    <TestSdkProvider client={client}>
      <RewriteBanner />
    </TestSdkProvider>,
  );
  return { client, ...utils };
}

describe("RewriteBanner", () => {
  it("renders nothing when no policies are pending", async () => {
    const listMigrationPending = vi.fn(async () => ({ ids: [] }));
    const { container } = renderBanner({ listMigrationPending });
    await waitFor(() => expect(listMigrationPending).toHaveBeenCalled());
    expect(container.textContent).toBe("");
  });

  it("renders a 'N policies need migration' banner when pending is non-empty", async () => {
    const listMigrationPending = vi.fn(async () => ({
      ids: ["dashboard::p1", "dashboard::p2"],
    }));
    renderBanner({ listMigrationPending });
    await screen.findByText(/2 policies need migration/i);
    // The literal `context.custom.*` phrase comes from the spec banner copy.
    expect(screen.getByText(/context\.custom\.\*/)).toBeTruthy();
  });

  it("renders '1 policy needs migration' (singular) for a single pending id", async () => {
    const listMigrationPending = vi.fn(async () => ({ ids: ["dashboard::p1"] }));
    renderBanner({ listMigrationPending });
    await screen.findByText(/1 policy needs migration/i);
  });

  it("clicking Rewrite calls rewrite → putRaw → ack in order, then refreshes the banner", async () => {
    let pending = ["dashboard::p1"];
    const listMigrationPending = vi.fn(async () => ({ ids: [...pending] }));
    const listManaged = vi.fn(async () => [
      managed(
        "dashboard::p1",
        '@id("dashboard::p1") forbid (principal, action, resource) when { context.totalInputUsd > 100 };',
      ),
    ]);
    const calls: string[] = [];
    const rewritePolicyToCustom = vi.fn(
      async (args: { id: string; text: string; knownFields: readonly string[] }) => {
        calls.push("rewrite");
        expect(args.id).toBe("dashboard::p1");
        // Banner must pass the policy text and the v0 known-field set.
        expect(args.text).toContain("context.totalInputUsd");
        expect(args.knownFields).toContain("totalInputUsd");
        return {
          id: args.id,
          rewritten:
            '@id("dashboard::p1") forbid (principal, action, resource) when { context.custom.totalInputUsd > 100 };',
          applied: true,
        };
      },
    );
    const putRaw = vi.fn(
      async (args: { id: string; text: string }) => {
        calls.push("putRaw");
        expect(args.id).toBe("dashboard::p1");
        expect(args.text).toContain("context.custom.totalInputUsd");
        return {
          policy: managed(args.id, args.text),
          catalog: { policies: [], enabled: [] },
        };
      },
    );
    const migrationAck = vi.fn(async (id: string) => {
      calls.push("ack");
      // Simulate the SW removing the id from pending.
      pending = pending.filter((p) => p !== id);
      return { id, remaining: pending };
    });
    renderBanner({
      listMigrationPending,
      listManaged,
      rewritePolicyToCustom,
      putRaw,
      migrationAck,
    });

    fireEvent.click(await screen.findByRole("button", { name: /Rewrite/i }));

    await waitFor(() => expect(migrationAck).toHaveBeenCalledWith("dashboard::p1"));
    expect(rewritePolicyToCustom).toHaveBeenCalledTimes(1);
    expect(putRaw).toHaveBeenCalledTimes(1);
    expect(migrationAck).toHaveBeenCalledTimes(1);
    expect(calls).toEqual(["rewrite", "putRaw", "ack"]);
    // After ack the banner refreshed and lists 0 pending — nothing left to render.
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /Rewrite/i })).toBeNull(),
    );
  });

  it("when rewrite returns applied:false the SW auto-acks; banner skips putRaw but still refreshes", async () => {
    let pending = ["dashboard::p1"];
    const listMigrationPending = vi.fn(async () => ({ ids: [...pending] }));
    const listManaged = vi.fn(async () => [
      managed(
        "dashboard::p1",
        '@id("dashboard::p1") forbid (principal, action, resource);',
      ),
    ]);
    const rewritePolicyToCustom = vi.fn(async (args: { id: string; text: string }) => {
      // Nothing to rewrite. The SW pops it off pending itself.
      pending = pending.filter((p) => p !== args.id);
      return { id: args.id, rewritten: args.text, applied: false };
    });
    const putRaw = vi.fn();
    const migrationAck = vi.fn();
    renderBanner({
      listMigrationPending,
      listManaged,
      rewritePolicyToCustom,
      putRaw,
      migrationAck,
    });
    fireEvent.click(await screen.findByRole("button", { name: /Rewrite/i }));
    await waitFor(() => expect(rewritePolicyToCustom).toHaveBeenCalled());
    // No persistence required when the rewrite was a no-op.
    expect(putRaw).not.toHaveBeenCalled();
    // No explicit ack: the SW already removed the id.
    expect(migrationAck).not.toHaveBeenCalled();
    // Banner refreshed → no rewrite button.
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /Rewrite/i })).toBeNull(),
    );
  });

  // Phase 7 codex carry-over K: previously `applied:false` was a silent
  // pass-through — the row vanished with no user feedback. The banner
  // now surfaces a "No rewrite needed" toast that auto-clears after a
  // few seconds.
  it("surfaces a 'No rewrite needed' info toast when rewrite returns applied:false", async () => {
    vi.useFakeTimers();
    try {
      let pending = ["dashboard::p1"];
      const listMigrationPending = vi.fn(async () => ({ ids: [...pending] }));
      const listManaged = vi.fn(async () => [
        managed(
          "dashboard::p1",
          '@id("dashboard::p1") forbid (principal, action, resource);',
        ),
      ]);
      const rewritePolicyToCustom = vi.fn(
        async (args: { id: string; text: string }) => {
          pending = pending.filter((p) => p !== args.id);
          return { id: args.id, rewritten: args.text, applied: false };
        },
      );
      renderBanner({
        listMigrationPending,
        listManaged,
        rewritePolicyToCustom,
      });

      // findByRole drains microtasks under fake timers via the
      // testing-library async helpers.
      const btn = await vi.waitFor(() =>
        screen.getByRole("button", { name: /Rewrite/i }),
      );
      fireEvent.click(btn);

      // The toast shows up after the rewrite resolves.
      const toast = await vi.waitFor(() =>
        screen.getByTestId("rewrite-banner-info"),
      );
      expect(toast.textContent).toMatch(/No rewrite needed/i);

      // After the auto-clear delay the toast is gone.
      await vi.advanceTimersByTimeAsync(3_500);
      expect(screen.queryByTestId("rewrite-banner-info")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("surfaces an inline error when rewrite throws", async () => {
    const listMigrationPending = vi.fn(async () => ({ ids: ["dashboard::p1"] }));
    const listManaged = vi.fn(async () => [
      managed("dashboard::p1", '@id("dashboard::p1") forbid (principal, action, resource);'),
    ]);
    const rewritePolicyToCustom = vi.fn(async () => {
      throw Object.assign(new Error("schema_invalid: bad"), { kind: "schema_invalid" });
    });
    const putRaw = vi.fn();
    const migrationAck = vi.fn();
    renderBanner({
      listMigrationPending,
      listManaged,
      rewritePolicyToCustom,
      putRaw,
      migrationAck,
    });
    fireEvent.click(await screen.findByRole("button", { name: /Rewrite/i }));
    await screen.findByText(/schema_invalid/i);
    expect(putRaw).not.toHaveBeenCalled();
    expect(migrationAck).not.toHaveBeenCalled();
  });

  it("does not render Rewrite for ids whose policy text isn't found in managed (storage drift)", async () => {
    const listMigrationPending = vi.fn(async () => ({
      ids: ["dashboard::ghost"],
    }));
    const listManaged = vi.fn(async () => [] as ManagedPolicy[]);
    renderBanner({ listMigrationPending, listManaged });
    // Banner still shows the count and explains the situation, but the
    // Rewrite button is omitted since there's no text to rewrite.
    await screen.findByText(/1 policy needs migration/i);
    await waitFor(() => expect(listManaged).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: /Rewrite/i })).toBeNull();
    // The id is still listed so the user can act on it manually.
    expect(screen.getByText(/dashboard::ghost/)).toBeTruthy();
  });
});
