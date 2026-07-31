export function getSpareHeaderStyle(isMobile: boolean): string {
  if (isMobile) {
    // Header (65px) + Navbar (48px) = 113px
    return "h-[calc(100dvh-113px)] w-[calc(100dvw)]";
  }
  return "h-[calc(100dvh-65px)] w-[calc(100dvw-58px)]";
}
