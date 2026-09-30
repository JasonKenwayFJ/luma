import { useEffect, useRef } from "react";
import { AttachmentMeta, ChatMessage, ChatSummary } from "../types";
import Message from "./Message";
import Composer from "./Composer";
import ConnectionStatus from "./ConnectionStatus";
import "./ChatView.scss";

interface Props {
    chat: ChatSummary;
    messages: ChatMessage[];
    connected: boolean;
    hasMore: boolean;
    loadingMore: boolean;
    onBack: () => void;
    onSend: (text: string, attachments: AttachmentMeta[]) => Promise<void>;
    onLoadMore: (oldestSentAt: string) => void;
}

function ChatView({
                      chat,
                      messages,
                      connected,
                      hasMore,
                      loadingMore,
                      onBack,
                      onSend,
                      onLoadMore,
                  }: Props) {
    const scrollRef = useRef<HTMLDivElement>(null);
    const bottomRef = useRef<HTMLDivElement>(null);
    const firstScroll = useRef(true);
    const prevScrollHeight = useRef<number | null>(null);

    useEffect(() => {
        if (prevScrollHeight.current !== null && scrollRef.current) {
            const diff = scrollRef.current.scrollHeight - prevScrollHeight.current;
            scrollRef.current.scrollTop += diff;
            prevScrollHeight.current = null;
            return;
        }
        bottomRef.current?.scrollIntoView({
            behavior: firstScroll.current ? "auto" : "smooth",
        });
        firstScroll.current = false;
    }, [messages.length]);

    const handleScroll = () => {
        const el = scrollRef.current;
        if (!el || loadingMore || !hasMore) return;
        if (el.scrollTop < 40 && messages.length > 0) {
            prevScrollHeight.current = el.scrollHeight;
            onLoadMore(messages[0].sentAt);
        }
    };

    return (
        <div className="screen">
            <header className="topbar">
                <button className="icon-btn" onClick={onBack} title="К списку чатов">
                    ←
                </button>
                <div>
                    <div className="topbar__title">{chat.title}</div>
                    <ConnectionStatus connected={connected} />
                </div>
            </header>

            <main className="chat" ref={scrollRef} onScroll={handleScroll}>
                {loadingMore && <div className="chat__loading">Загрузка истории...</div>}
                {!hasMore && messages.length > 0 && (
                    <div className="chat__empty">Начало переписки</div>
                )}
                {messages.length === 0 && !loadingMore && (
                    <div className="chat__empty">Сообщений пока нет</div>
                )}
                {messages.map((m) => (
                    <Message key={m.id} message={m} />
                ))}
                <div ref={bottomRef} />
            </main>

            <Composer connected={connected} onSend={onSend} />
        </div>
    );
}

export default ChatView;