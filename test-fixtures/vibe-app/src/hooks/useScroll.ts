export function useScroll(): void {
  window.addEventListener('scroll', () => {
    console.log('scrolled');
  });
}

export function useMousePosition(): { x: number; y: number } {
  return { x: 0, y: 0 };
}
