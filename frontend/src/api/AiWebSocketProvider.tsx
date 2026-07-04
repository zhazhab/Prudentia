import { useEffect, useMemo, useState, type ReactNode } from "react";
import { AiWebSocketSession } from "./aiWs";
import { AiWebSocketContext } from "./aiWsContext";

export function AiWebSocketProvider({
  children,
  createSession = () => new AiWebSocketSession()
}: {
  children: ReactNode;
  createSession?: () => AiWebSocketSession;
}) {
  const [session] = useState(createSession);
  const [connectionError, setConnectionError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    session
      .connect()
      .then(() => {
        if (active) {
          setConnectionError(null);
        }
      })
      .catch((error) => {
        if (active) {
          setConnectionError(error instanceof Error ? error.message : String(error));
        }
      });

    return () => {
      active = false;
      session.close();
    };
  }, [session]);

  const value = useMemo(() => ({ session, connectionError }), [session, connectionError]);

  return <AiWebSocketContext.Provider value={value}>{children}</AiWebSocketContext.Provider>;
}
