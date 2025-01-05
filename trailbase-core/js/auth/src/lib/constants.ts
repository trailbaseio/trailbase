export const DEV_MODE: boolean = import.meta.env.DEV;

export const HOST = DEV_MODE ? "http://localhost:4000" : "";
export const AUTH_API = `${HOST}/api/auth/v1`;
export const RECORD_API = `${HOST}/api/records/v1`;

export const INPUT_STYLE = "outline outline-1 rounded p-2";

export const BUTTON_STYLE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "whitespace-nowrap",
  "rounded-md",
  "text-sm",
  "font-medium",
  "ring-offset-background",
  "transition-colors",
  "focus-visible:outline-none",
  "focus-visible:ring-2",
  "focus-visible:ring-ring",
  "focus-visible:ring-offset-2",
  "disabled:pointer-events-none",
  "disabled:opacity-50",
  "text-primary-foreground",
  "bg-primary",
  "hover:bg-primary/90",
  "h-10",
  "px-4",
  "py-2",
];

export const OUTLINE_BUTTON_STYLE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "whitespace-nowrap",
  "rounded-md",
  "text-sm",
  "font-medium",
  "ring-offset-background",
  "transition-colors",
  "focus-visible:outline-none",
  "focus-visible:ring-2",
  "focus-visible:ring-ring",
  "focus-visible:ring-offset-2",
  "disabled:pointer-events-none",
  "disabled:opacity-50",
  "border",
  "border-input",
  "hover:bg-accent",
  "hover:text-accent-foreground",
  "h-10",
  "px-4",
  "py-2",
];

export const ICON_STYLE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "rounded-md",
  "p-2",
  "hover:text-primary-foreground",
  "hover:bg-primary/90",
];

export const DESTRUCTIVE_ICON_STYLE = [
  "inline-flex",
  "items-center",
  "justify-center",
  "rounded-md",
  "p-2",
  "hover:text-primary-foreground",
  "hover:bg-destructive/90",
];
