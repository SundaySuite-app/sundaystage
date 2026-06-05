/**
 * Cross-app service-item kind vocabulary — the pure glue the Plan→Stage bridge
 * needs to turn an incoming SundayPlan `ServicePlan` into Stage cues.
 *
 * This **mirrors `sunday-platform` `packages/contracts/src/mapping.ts`** (the
 * canonical kind-mapping shared across the suite). It is vendored verbatim here
 * — minus the `zod` runtime, which this repo does not depend on — because Stage
 * consumes the contracts as plain types over the wire, not as a published npm
 * package. Converge onto the published `@sunday/contracts` export once it ships;
 * until then keep this byte-for-byte in step with the platform mapping so no
 * bridge invents its own vocabulary.
 *
 * Producers (Plan) map their local kind → canonical when emitting a plan;
 * consumers (Stage) map canonical → their own rendering vocabulary. Unknown
 * inputs map to `custom` (forward-compatible: a new app-side kind never throws,
 * it degrades to a generic slide).
 */

/**
 * Canonical running-order item kind — the wire superset every app maps onto.
 * Mirrors `ServiceItemKind` in the platform `service.ts` contract.
 */
export type ServiceItemKind =
  | "song"
  | "scripture"
  | "sermon"
  | "reading"
  | "prayer"
  | "offering"
  | "announcement"
  | "welcome"
  | "response"
  | "media"
  | "gap"
  | "custom";

/** SundayPlan's local kinds (template_item ∪ service_item, migration 0002). */
export type PlanServiceItemKind =
  | "welcome"
  | "worship_set"
  | "song"
  | "scripture"
  | "sermon"
  | "response"
  | "closing"
  | "announcement"
  | "gap";

/** SundayStage's local kinds (service_item, sql/0001_initial). */
export type StageServiceItemKind =
  | "song"
  | "scripture"
  | "custom_deck"
  | "video"
  | "announcement"
  | "gap";

const PLAN_TO_CANONICAL: Record<PlanServiceItemKind, ServiceItemKind> = {
  welcome: "welcome",
  worship_set: "song",
  song: "song",
  scripture: "scripture",
  sermon: "sermon",
  response: "response",
  closing: "custom",
  announcement: "announcement",
  gap: "gap",
};

/** How each canonical kind is presented in SundayStage (its local vocabulary). */
const CANONICAL_TO_STAGE: Record<ServiceItemKind, StageServiceItemKind> = {
  song: "song",
  scripture: "scripture",
  sermon: "custom_deck",
  reading: "scripture",
  prayer: "custom_deck",
  offering: "custom_deck",
  announcement: "announcement",
  welcome: "custom_deck",
  response: "custom_deck",
  media: "video",
  gap: "gap",
  custom: "custom_deck",
};

/** Map a SundayPlan kind to the canonical kind (unknown → `custom`). */
export function serviceItemKindFromPlan(kind: string): ServiceItemKind {
  // `kind` is untrusted (from another app's payload). Only OWN keys count, so a
  // lookup of "constructor"/"toString"/… returns "custom" instead of leaking an
  // inherited Object.prototype member where a ServiceItemKind is promised.
  return Object.prototype.hasOwnProperty.call(PLAN_TO_CANONICAL, kind)
    ? PLAN_TO_CANONICAL[kind as PlanServiceItemKind]
    : "custom";
}

/** Map a canonical kind to SundayStage's rendering vocabulary. */
export function serviceItemKindToStage(
  kind: ServiceItemKind,
): StageServiceItemKind {
  return CANONICAL_TO_STAGE[kind];
}

/** Convenience: SundayPlan kind → SundayStage rendering vocabulary in one hop. */
export function planKindToStage(kind: string): StageServiceItemKind {
  return serviceItemKindToStage(serviceItemKindFromPlan(kind));
}
