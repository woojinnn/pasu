import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import {
  createListing,
  listManagedPolicies,
  type CreateListingBody,
  type ListingKind,
  type MarketSeverity,
  type SetMember,
} from "../../server-api";

import "../market.css";

/** Per-kind input the editor passes in. The modal reads it as-is and asks
 *  the user for the marketplace metadata (slug, displayName, domain, etc.). */
export type PublishSource =
  | {
      kind: "policy";
      cedarText: string;
      manifest?: unknown;
      policyTree?: string | null;
      suggestedDisplayName: string;
      suggestedSlug: string;
    }
  | {
      kind: "set";
      suggestedDisplayName: string;
      suggestedSlug: string;
      description?: string;
      /** Local dashboard:: ids of member policies. The modal looks them up
       *  in the SW list to snapshot cedar_text/manifest at publish time. */
      memberIds: readonly string[];
    };

export interface PublishModalProps {
  open: boolean;
  onClose: () => void;
  source: PublishSource | null;
}

const SEMVER_RE = /^[0-9]+\.[0-9]+\.[0-9]+$/;
const SLUG_RE = /^[A-Za-z0-9_./()-]{1,128}$/;
const DOMAINS = [
  "security", "swap", "perp", "lending", "nft", "airdrop",
  "portfolio", "ammlp", "bridge", "sale", "staking", "gov",
];

/**
 * Publish modal — collects marketplace metadata (slug, display name,
 * domain, severity for policy; just slug + name for set) and POSTs to
 * `/market/listings`. On success it navigates to `/market/:slug` so the
 * publisher lands on the live detail page of their fresh listing.
 *
 * For set publishes the modal snapshots member policies' cedar_text /
 * manifest from the SW at the moment of publish. The marketplace gets a
 * self-contained payload; no FK to the editor copy is kept.
 */
export function PublishModal({ open, onClose, source }: PublishModalProps) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const policiesQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
    enabled: open && source?.kind === "set",
  });

  const [slug, setSlug] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [description, setDescription] = useState("");
  const [version, setVersion] = useState("1.0.0");
  const [domain, setDomain] = useState("security");
  const [severity, setSeverity] = useState<MarketSeverity>("warn");
  const [intentsRaw, setIntentsRaw] = useState("");

  // Seed inputs when the modal opens with a fresh source.
  if (open && source && slug === "" && displayName === "") {
    setSlug(source.suggestedSlug);
    setDisplayName(source.suggestedDisplayName);
    if (source.kind === "set") setDescription(source.description ?? "");
  }

  const reset = () => {
    setSlug("");
    setDisplayName("");
    setDescription("");
    setVersion("1.0.0");
    setDomain("security");
    setSeverity("warn");
    setIntentsRaw("");
  };

  const publishMut = useMutation({
    mutationFn: async (): Promise<{ slug: string; kind: ListingKind }> => {
      if (!source) throw new Error("no source");
      const trimSlug = slug.trim();
      const trimName = displayName.trim();
      if (!SLUG_RE.test(trimSlug)) {
        throw new Error("slug 형식이 잘못됐습니다 (영문/숫자/_.-()/ 만, 1-128자)");
      }
      if (trimName.length === 0) throw new Error("표시 이름이 필요합니다.");
      if (!SEMVER_RE.test(version)) throw new Error("버전은 MAJOR.MINOR.PATCH 형식이어야 합니다.");

      if (source.kind === "policy") {
        const intents = parseIntents(intentsRaw);
        const body: CreateListingBody = {
          slug: trimSlug,
          kind: "policy",
          display_name: { en: trimName, ko: trimName },
          description: description.trim()
            ? { en: description.trim(), ko: description.trim() }
            : undefined,
          domain,
          intents: intents.length > 0 ? intents : undefined,
          severity,
          version,
          cedar_text: source.cedarText,
          manifest: source.manifest,
          policy_tree: source.policyTree ?? undefined,
        };
        await createListing(body);
        return { slug: trimSlug, kind: "policy" };
      }

      // set: snapshot member policies from the SW list
      const policies = policiesQ.data ?? [];
      const byId = new Map(policies.map((p) => [p.id, p]));
      const members: SetMember[] = [];
      for (const mid of source.memberIds) {
        const p = byId.get(mid);
        if (!p) continue; // stale reference — silently drop
        members.push({
          slug: stripPrefix(p.id),
          display_name: p.displayName ?? stripPrefix(p.id),
          cedar_text: p.text,
          manifest: p.manifest,
        });
      }
      if (members.length === 0) {
        throw new Error("발행할 멤버 정책이 없습니다.");
      }
      const body: CreateListingBody = {
        slug: trimSlug,
        kind: "set",
        display_name: { en: trimName, ko: trimName },
        description: description.trim()
          ? { en: description.trim(), ko: description.trim() }
          : undefined,
        version,
        members,
      };
      await createListing(body);
      return { slug: trimSlug, kind: "set" };
    },
    onSuccess: async ({ slug: createdSlug }) => {
      await qc.invalidateQueries({ queryKey: ["market-listings"] });
      reset();
      onClose();
      navigate(`/market/${encodeURIComponent(createdSlug)}`);
    },
  });

  if (!open || !source) return null;

  return (
    <div
      className="publish-modal-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget && !publishMut.isPending) {
          reset();
          onClose();
        }
      }}
    >
      <div className="publish-modal">
        <header className="publish-modal-head">
          <h2>{source.kind === "set" ? "패키지 발행" : "정책 발행"}</h2>
        </header>
        <div className="publish-modal-body">
          <Field label="슬러그 (URL용)" hint="영문/숫자/_.-()/ 만, 한 번 발행하면 변경 불가">
            <input
              type="text"
              value={slug}
              onChange={(e) => setSlug(e.target.value)}
              maxLength={128}
            />
          </Field>
          <Field label="표시 이름">
            <input
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              maxLength={120}
            />
          </Field>
          <Field label="설명 (선택)">
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              maxLength={500}
            />
          </Field>
          <Field label="버전 (SemVer)" hint="MAJOR.MINOR.PATCH 형식, 예: 1.0.0">
            <input
              type="text"
              value={version}
              onChange={(e) => setVersion(e.target.value)}
            />
          </Field>

          {source.kind === "policy" && (
            <>
              <Field label="도메인">
                <select value={domain} onChange={(e) => setDomain(e.target.value)}>
                  {DOMAINS.map((d) => (
                    <option key={d} value={d}>{d}</option>
                  ))}
                </select>
              </Field>
              <Field label="심각도">
                <select
                  value={severity}
                  onChange={(e) => setSeverity(e.target.value as MarketSeverity)}
                >
                  <option value="warn">Warn (경고)</option>
                  <option value="deny">Deny (차단)</option>
                </select>
              </Field>
              <Field label="의도 태그 (쉼표 구분, 선택)" hint="예: slippage, sandwich">
                <input
                  type="text"
                  value={intentsRaw}
                  onChange={(e) => setIntentsRaw(e.target.value)}
                  placeholder="slippage, sandwich"
                />
              </Field>
            </>
          )}

          {source.kind === "set" && (
            <div style={{ fontSize: 12.5, color: "var(--slate-500)" }}>
              {policiesQ.isLoading
                ? "멤버 정책 정보 불러오는 중…"
                : `${source.memberIds.length}개 멤버 정책 스냅샷이 함께 발행됩니다.`}
            </div>
          )}

          {publishMut.isError && (
            <div className="publish-error">{(publishMut.error as Error).message}</div>
          )}
        </div>
        <footer className="publish-modal-foot">
          <button
            type="button"
            className="btn-secondary"
            disabled={publishMut.isPending}
            onClick={() => {
              reset();
              onClose();
            }}
          >
            취소
          </button>
          <button
            type="button"
            className="btn-primary"
            disabled={publishMut.isPending}
            onClick={() => publishMut.mutate()}
          >
            {publishMut.isPending ? "발행 중…" : "발행"}
          </button>
        </footer>
      </div>
    </div>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="set-field">
      <span className="set-field-label">{label}</span>
      {children}
      {hint && <span className="set-field-hint">{hint}</span>}
    </label>
  );
}

function parseIntents(raw: string): string[] {
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function stripPrefix(id: string): string {
  const PREFIX = "dashboard::";
  return id.startsWith(PREFIX) ? id.slice(PREFIX.length) : id;
}
