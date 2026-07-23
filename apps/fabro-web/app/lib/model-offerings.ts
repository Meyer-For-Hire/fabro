import type { Model } from "@qltysh/fabro-api-client";

type ModelOfferingIdentity = Pick<Model, "provider" | "id">;

export function modelOfferingKey(model: ModelOfferingIdentity): string {
  return `${model.provider}\u0000${model.id}`;
}

export function modelOfferingTestArgs(
  model: ModelOfferingIdentity,
): [id: string, provider: string] {
  return [model.id, model.provider];
}
