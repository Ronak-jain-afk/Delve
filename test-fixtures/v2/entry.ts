import { run } from './consumer';
import { useWildcard } from './wildcard_consumer';
import { isActive } from './type_consumer';

export function main(): void {
  run();
  useWildcard();
  isActive("active");
}

main();
