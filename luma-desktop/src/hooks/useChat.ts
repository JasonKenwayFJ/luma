import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AttachmentMeta, ChatMessage, WireMessage } from "../types";

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

        const applyHistory = (json: string) => {
            let wires: WireMessage[];
            try {
                wires = JSON.parse(json);
            } catch {
                return;
            }
            setMessages((prev) => mergeMessages(prev, wires.map(toChat)));
        };

        const unlistenMessage = listen<string>("ws-message", (event) => {
            let wire: WireMessage;
            try {
                wire = JSON.parse(event.payload);
            } catch {
                return;
            }
            setMessages((prev) => mergeMessages(prev, [toChat(wire)]));
        });

        const unlistenHistory = listen<string>("ws-history", (event) => {
            applyHistory(event.payload);
        });

        const unlistenStatus = listen<boolean>("ws-status", (event) => {
            setConnected(event.payload);
        });

        // Разовые запросы: события до подписки потеряны, Rust их помнит.
        invoke<boolean>("get_connection_status").then(setConnected);
        invoke<string | null>("get_history").then((json) => {
            if (json) applyHistory(json);
        });

        return () => {
            unlistenMessage.then((f) => f());
            unlistenHistory.then((f) => f());
            unlistenStatus.then((f) => f());
        };
    }, [userId]);

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

            await invoke("send_message", { text: JSON.stringify(wire) });
            setMessages((prev) => mergeMessages(prev, [{ ...wire, own: true }]));
        },
        [userId]
    );

    return { messages, connected, send };
}