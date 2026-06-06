import type { User, Status } from './types';
import { API_VERSION } from './types';

export function getVersion(): number {
  return API_VERSION;
}

export function isActive(status: Status): boolean {
  return status === "active";
}
