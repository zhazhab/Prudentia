import { useContext } from "react";
import { AiWebSocketContext } from "./aiWsContext";

export function useAiWebSocket() {
  const value = useContext(AiWebSocketContext);
  if (!value) {
    throw new Error("useAiWebSocket must be used inside AiWebSocketProvider");
  }
  return value;
}
