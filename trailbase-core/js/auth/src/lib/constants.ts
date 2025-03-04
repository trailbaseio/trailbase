import { buttonVariants } from "@/components/ui/button";
import { textFieldInputStyle } from "@/components/ui/text-field";

export const DEV_MODE: boolean = import.meta.env.DEV;

export const HOST = DEV_MODE ? "http://localhost:4000" : "";
export const AUTH_API = `${HOST}/api/auth/v1`;
export const RECORD_API = `${HOST}/api/records/v1`;

export const INPUT_STYLE = textFieldInputStyle;

export const BUTTON_STYLE = buttonVariants({ variant: "default" });
export const OUTLINE_BUTTON_STYLE = buttonVariants({ variant: "outline" });
