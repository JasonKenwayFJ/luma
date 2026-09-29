import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AttachmentMeta, ChatMessage, ServerFrame, WireMessage } from "../types";

function mergeMessages(prev: ChatMessage[], incoming: ChatMessage[]): ChatMessage[] {
    const known = new Set(prev.map((m) => m.id));
    const fresh = incoming.filter((m) => !known.has(m.id));
    if (fresh.length === 0) return prev;
    return [...prev, ...fresh].sort((a, b) => a.sentAt.localeCompare(b.sentAt));
}

interface SendParams {
    roomId: string;
    authorName: string;
    text: string;
    attachments: AttachmentMeta[];
}

export function useChat(userId: string) {
    const [messages, setMessages] = useState<ChatMessage[]>([]);
    const [connected, setConnected] = useState(false);

    useEffect(() => {
        const toChat = (wire: WireMessage): ChatMessage => ({
            ...wire,
            own: wire.userId === userId,
        });

        // Один и тот же разбор кадра используется и для живых событий,
        // и для "подобранного" состояния при монтировании (get_history) —
        // не хочется держать эту логику в двух местах.
        const applyFrame = (raw: string) => {
            let frame: ServerFrame;
            try {
                frame = JSON.parse(raw);
            } catch {
                return;
            }
            if (frame.type === "message") {
                const { type: _type, ...wire } = frame;
                setMessages((prev) => mergeMessages(prev, [toChat(wire as WireMessage)]));
            } else if (frame.type === "history" || frame.type === "summary") {
                setMessages((prev) => mergeMessages(prev, frame.messages.map(toChat)));
            }
        };

        const unlistenFrame = listen<string>("ws-frame", (event) => applyFrame(event.payload));
        const unlistenStatus = listen<boolean>("ws-status", (event) => setConnected(event.payload));

        invoke<boolean>("get_connection_status").then(setConnected);
        invoke<string | null>("get_history").then((json) => {
            if (json) applyFrame(json);
        });

        return () => {
            unlistenFrame.then((f) => f());
            unlistenStatus.then((f) => f());
        };
    }, [userId]);

    const joinRoom = useCallback((roomId: string) => {
        invoke("send_message", { text: JSON.stringify({ type: "join", roomId }) });
    }, []);

    const leaveRoom = useCallback(() => {
        invoke("send_message", { text: JSON.stringify({ type: "leave" }) });
    }, []);

    const send = useCallback(
        async ({ roomId, authorName, text, attachments }: SendParams) => {
            const wire: WireMessage = {
                id: crypto.randomUUID(),
                userId,
                roomId,
                authorName,
                text,
                ...(attachments.length > 0 ? { attachments } : {}),
                sentAt: new Date().toISOString(),
            };

            await invoke("send_message", { text: JSON.stringify({ type: "send", ...wire }) });
            setMessages((prev) => mergeMessages(prev, [{ ...wire, own: true }]));
        },
        [userId]
    );

    return { messages, connected, send, joinRoom, leaveRoom };
}