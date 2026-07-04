import { createContext } from "react";
import type { AiWebSocketSession } from "./aiWs";

export type AiWebSocketContextValue = {
  session: AiWebSocketSession;
  connectionError: string | null;
};

export const AiWebSocketContext = createContext<AiWebSocketContextValue | null>(null);
