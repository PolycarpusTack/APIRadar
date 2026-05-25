import { usersApi } from "../generated/api";

export async function resolveUserPhone(userId: string): Promise<string | null> {
  const response = await usersApi.getUserById(userId);
  return response.phone ?? null;
}

export async function buildUserSummary(userId: string) {
  const response = await usersApi.getUserById(userId);
  return {
    id: response.id,
    email: response.email,
    phone: response.phone,
    displayName: response.name,
  };
}
