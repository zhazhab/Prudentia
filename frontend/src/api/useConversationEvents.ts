import { useEffect, useState } from "react";
import { ConversationEventClient } from "./conversationEvents";

export function useConversationEvents() {
  const [client] = useState(() => new ConversationEventClient());
  const [connectionError, setConnectionError] = useState<string | null>(null);

  useEffect(() => {
    const unsubscribe = client.onConnection(setConnectionError);
    client.connect();
    return () => {
      unsubscribe();
      client.close();
    };
  }, [client]);

  return { client, connectionError };
}
