import { formatDate } from './utils/formatDate';
import { useScroll } from './hooks/useScroll';

export function main() {
  console.log('App started');
  formatDate(new Date());
  useScroll();
}

main();
