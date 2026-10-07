import {useEffect, useRef} from "react";
import {AttachmentMeta, ChatMessage, ChatSummary} from "../types";
import Message from "./Message";
import Composer from "./Composer";
import ConnectionStatus from "./ConnectionStatus";
import { useCall } from "../hooks/useCall";
import type { RTCSignal } from "../hooks/useChat";
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
    userId: string;
    sendSignal: (to: string, roomId: string, signal: RTCSignal) => void;
    onSignal: (handler: (frame: { from: string; roomId: string; signal: RTCSignal }) => void) => () => void;
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
                      userId,
                      sendSignal,
                      onSignal,
                  }: Props) {
    const scrollRef = useRef<HTMLDivElement>(null);
    const bottomRef = useRef<HTMLDivElement>(null);
    const firstScroll = useRef(true);
    const prevScrollHeight = useRef<number | null>(null);
    const call = useCall(chat.id, userId, onSignal, sendSignal);
    const localVideo = useRef<HTMLVideoElement>(null);
    const remoteVideo = useRef<HTMLVideoElement>(null);

    useEffect(() => { if (localVideo.current) localVideo.current.srcObject = call.localStream; }, [call.localStream]);
    useEffect(() => { if (remoteVideo.current) remoteVideo.current.srcObject = call.remoteStream; }, [call.remoteStream]);

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
                <div className={"topbar__content"}>


                    <button className="icon-btn" onClick={onBack} title="К списку чатов">
                        ←
                    </button>
                    <div>
                        <div className="topbar__title">{chat.title}</div>
                        <ConnectionStatus connected={connected}/>
                    </div>
                    <div className="toolbar__call">
                        <button onClick={() => void call.start(false)} title="Аудиозвонок">📞</button>
                        <button onClick={() => void call.start(true)} title="Видеозвонок">🎥</button>
                    </div>
                </div>
            </header>

            {(call.incoming || call.active || call.error) && <section className="call-panel">
                {call.incoming && <div>{call.incoming} звонит вам <button onClick={() => void call.answer()}>Ответить</button></div>}
                {call.active && <>
                    <video ref={remoteVideo} autoPlay playsInline className="call-panel__remote" />
                    <video ref={localVideo} autoPlay playsInline muted className="call-panel__local" />
                    <button onClick={() => void call.shareScreen()}>Показать экран</button>
                    <button className="call-panel__hangup" onClick={call.hangup}>Завершить</button>
                </>}
                {call.error && <div className="call-panel__error">{call.error}</div>}
            </section>}

            <main className="chat" ref={scrollRef} onScroll={handleScroll}>
                {loadingMore && <div className="chat__loading">Загрузка истории...</div>}
                {!hasMore && messages.length > 0 && (
                    <div className="chat__empty">Начало переписки</div>
                )}
                {messages.length === 0 && !loadingMore && (
                    <div className="chat__empty">Сообщений пока нет</div>
                )}
                {messages.map((m) => (
                    <Message key={m.id} message={m}/>
                ))}
                <div ref={bottomRef}/>
            </main>
            <Composer connected={connected} onSend={onSend}/>
        </div>
    );
}

export default ChatView;
