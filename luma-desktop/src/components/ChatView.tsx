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
    onBack: () => void;
    onSend: (text: string, attachments: AttachmentMeta[]) => Promise<void>;
}

function ChatView({ chat, messages, connected, onBack, onSend }: Props) {
    const bottomRef = useRef<HTMLDivElement>(null);
    const firstScroll = useRef(true);

    // При входе в чат прыгаем вниз мгновенно, дальше скроллим плавно.
    useEffect(() => {
        bottomRef.current?.scrollIntoView({
            behavior: firstScroll.current ? "auto" : "smooth",
        });
        firstScroll.current = false;
    }, [messages.length]);

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

            <main className="chat">
                {messages.length === 0 && (
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