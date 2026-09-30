import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AttachmentMeta, ChatMessage, ServerFrame, UserProfile, WireMessage } from "../types";

function mergeMessages(prev: ChatMessage[], incoming: ChatMessage[]): ChatMessage[] {
    const known = new Set(prev.map((m) => m.id));
    const fresh = incoming.filter((m) => !known.has(m.id));
    if (fresh.length === 0) return prev;
    return [...prev, ...fresh].sort((a, b) => a.sentAt.localeCompare(b.sentAt));
}

interface SendParams {
    roomId: string;
    text: string;
    attachments: AttachmentMeta[];
}

export function useChat(profile: UserProfile | null) {
    const [messages, setMessages] = useState<ChatMessage[]>([]);
    const [connected, setConnected] = useState(false);
    const [hasMoreByRoom, setHasMoreByRoom] = useState<Record<string, boolean>>({});
    const [loadingMore, setLoadingMore] = useState(false);

    const profileRef = useRef(profile);
    profileRef.current = profile;

    // Подписки на события живут всё время работы приложения — вне
    // зависимости от того, залогинен пользователь сейчас или нет.
    useEffect(() => {
        const toChat = (wire: WireMessage): ChatMessage => ({
            ...wire,
            own: wire.userId === profileRef.current?.userId,
        });

        const applyFrame = (raw: string) => {
            let frame: ServerFrame;
            try {
                frame = JSON.parse(raw);
            } catch {
                return;
            }

            if (frame.type === "message" || frame.type === "preview") {
                const { type: _type, ...wire } = frame;
                setMessages((prev) => mergeMessages(prev, [toChat(wire as WireMessage)]));
            } else if (frame.type === "summary") {
                setMessages((prev) => mergeMessages(prev, frame.messages.map(toChat)));
            } else if (frame.type === "history") {
                setMessages((prev) => mergeMessages(prev, frame.messages.map(toChat)));
                setHasMoreByRoom((prev) => ({ ...prev, [frame.roomId]: frame.hasMore }));
                if (!frame.isInitial) setLoadingMore(false);
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
    }, []);

    // А вот само соединение живёт только пока пользователь залогинен:
    // логин подключает сокет с токеном, логаут рвёт его.
    useEffect(() => {
        if (profile) {
            invoke("connect_ws", { token: profile.token });
        } else {
            invoke("disconnect_ws");
            setMessages([]);
        }
    }, [profile]);

    const joinRoom = useCallback((roomId: string) => {
        invoke("send_message", { text: JSON.stringify({ type: "join", roomId }) });
    }, []);

    const leaveRoom = useCallback(() => {
        invoke("send_message", { text: JSON.stringify({ type: "leave" }) });
    }, []);

    const loadMore = useCallback((roomId: string, oldestSentAt: string) => {
        setLoadingMore(true);
        invoke("send_message", {
            text: JSON.stringify({ type: "loadMore", roomId, beforeSentAt: oldestSentAt }),
        });
    }, []);

    const send = useCallback(async ({ roomId, text, attachments }: SendParams) => {
        const p = profileRef.current;
        if (!p) return;

        // userId, authorName и sentAt всё равно перезапишет сервер — здесь они
        // нужны только для мгновенного локального отображения своего сообщения.
        const wire: WireMessage = {
            id: crypto.randomUUID(),
            userId: p.userId,
            roomId,
            authorName: p.username,
            text,
            ...(attachments.length > 0 ? { attachments } : {}),
            sentAt: new Date().toISOString(),
        };

        await invoke("send_message", { text: JSON.stringify({ type: "send", ...wire }) });
        setMessages((prev) => mergeMessages(prev, [{ ...wire, own: true }]));
    }, []);

    return { messages, connected, hasMoreByRoom, loadingMore, send, joinRoom, leaveRoom, loadMore };
}