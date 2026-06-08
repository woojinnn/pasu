/**
 * Install (copy-to-editor) a market listing into the local extension store.
 * Shared by the detail page and the in-list "받기" modal so both paths copy
 * the cedar/manifest into chrome.storage identically.
 */
import {
  dashboardId,
  dashboardSetId,
  installListing,
  listManagedPolicies,
  pickI18n,
  putPolicy,
  putPolicySet,
  type ListingDetail,
} from "../server-api";

/** Find an unused local dashboard id, suffixing `-2`, `-3`, … on collision. */
function freshLocalId(preferredSlug: string, existing: Set<string>, kind: "policy" | "set"): string {
  const make = kind === "policy" ? dashboardId : dashboardSetId;
  const sanitized = preferredSlug.replace(/[^A-Za-z0-9_./()-]/g, "-").slice(0, 96);
  if (!existing.has(make(sanitized))) return make(sanitized);
  for (let i = 2; i < 1000; i++) {
    const candidate = `${sanitized}-${i}`;
    if (!existing.has(make(candidate))) return make(candidate);
  }
  return make(`${sanitized}-${existing.size}`);
}

/** Copy a listing's latest version into the editor. Returns the new local id. */
export async function installListingToEditor(
  detail: ListingDetail,
  locale: "ko" | "en",
): Promise<{ kind: "policy" | "set"; id: string }> {
  if (!detail.latest_version || !detail.current_version) {
    throw new Error(
      locale === "ko" ? "이 listing에는 발행된 버전이 없습니다." : "This listing has no published version.",
    );
  }
  const body = await installListing(detail.id, detail.current_version);
  const existing = await listManagedPolicies();
  const existingIds = new Set(existing.map((p) => p.id));
  const cat = detail.category ?? detail.domain ?? undefined;

  if (detail.kind === "policy") {
    if (!body.cedar_text) throw new Error("server returned policy version without cedar_text");
    const id = freshLocalId(detail.slug, existingIds, "policy");
    await putPolicy({
      id,
      cedarText: body.cedar_text,
      manifest: body.manifest,
      displayName: pickI18n(detail.display_name, locale) || detail.slug,
      source: "market",
      sourceListingId: detail.id,
      sourceVersion: detail.current_version,
      cat,
      life: "publish",
    });
    return { kind: "policy", id };
  }

  const members = body.members ?? [];
  if (members.length === 0) throw new Error("server returned set version without members");
  const memberIds: string[] = [];
  for (const m of members) {
    const id = freshLocalId(m.slug, existingIds, "policy");
    await putPolicy({
      id,
      cedarText: m.cedar_text,
      manifest: m.manifest,
      displayName: m.display_name || m.slug,
      source: "market",
      sourceListingId: detail.id,
      sourceVersion: detail.current_version,
      cat,
      life: "publish",
    });
    existingIds.add(id);
    memberIds.push(id);
  }
  const setId = freshLocalId(detail.slug, new Set(), "set");
  await putPolicySet({
    id: setId,
    displayName: pickI18n(detail.display_name, locale) || detail.slug,
    description: pickI18n(detail.description, locale) || undefined,
    memberIds,
    source: "market",
    readOnly: true,
    sourceListingId: detail.id,
    sourceVersion: detail.current_version,
    cat,
  });
  return { kind: "set", id: setId };
}
