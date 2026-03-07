export const levels = ["error", "warning", "success", "info"] as const;
export type Level = (typeof levels)[number];

export function levelToStyle(level: Level): string {
  switch (level) {
    case "error":
      return "bg-error text-error-foreground";
    case "warning":
      return "bg-warning text-warning-foreground";
    case "info":
      return "bg-info text-info-foreground";
    case "success":
      return "bg-success text-success-foreground";
  }
}
