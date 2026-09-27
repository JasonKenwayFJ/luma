import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChatMessage, WireMessage } from "./types";
import {
    getOrCreateUserId,
    getOrCreateUserName,
    setUserName,
    formatTime,
} from "./utils";
import "./App.scss";
import Message from "./components/Message.tsx";

const ROOM_ID = "general";

function App() {
    const [messages, setMessages] = useState<ChatMessage[]>([]);
    const [text, setText] = useState("");
    const [name, setName] = useState(getOrCreateUserName());
    const bottomRef = useRef<HTMLDivElement>(null);


    const myUserIdRef = useRef(getOrCreateUserId());

    useEffect(() => {
        const unlisten = listen<string>("ws-message", (event) => {
            let wire: WireMessage;
            try {
                wire = JSON.parse(event.payload);
            } catch {
                return;
            }
            setMessages((prev) => [
                ...prev,
                { ...wire, own: wire.userId === myUserIdRef.current },
            ]);
        });
        return () => {
            unlisten.then((f) => f());
        };
    }, []);

    useEffect(() => {
        bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [messages.length]);

    const send = async () => {
        const trimmed = text.trim();
        if (!trimmed) return;

        const wire: WireMessage = {
            id: crypto.randomUUID(),
            userId: myUserIdRef.current,
            roomId: ROOM_ID,
            authorName: name,
            text: trimmed,
            sentAt: new Date().toISOString(),
        };


        await invoke("send_message", { text: JSON.stringify(wire) });

        
        setMessages((prev) => [...prev, { ...wire, own: true }]);
        setText("");
    };

    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
        }
    };

    const handleNameChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        setName(e.target.value);
        setUserName(e.target.value);
    };

    return (
        <div className="app">
            <header className="app__header">
                <span className="app__title">Luma</span>
                <input
                    className="app__name-input"
                    value={name}
                    onChange={handleNameChange}
                    maxLength={20}
                    title="Ваше имя в чате"
                />
            </header>

            <main className="chat">
                {messages.length === 0 && (
                    <div className="chat__empty">Сообщений пока нет</div>
                )}

                {messages.map((m) => (
                    <div key={m.id} className={`row ${m.own ? "row--own" : "row--other"}`}>
                        {messages.map((m) => (
                            <Message key={m.id} message={m} />
                        ))}
                        <div className={`bubble ${m.own ? "bubble--own" : "bubble--other"}`}>
                            {!m.own && <div className="bubble__author">{m.authorName}</div>}
                            <div className="bubble__text">{m.text}</div>
                            <div className="bubble__time">{formatTime(m.sentAt)}</div>
                        </div>
                    </div>
                ))}
                <div ref={bottomRef} />
            </main>

            <footer className="composer">
                <input
                    className="composer__input"
                    value={text}
                    onChange={(e) => setText(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder="Напишите сообщение..."
                />
                <button className="composer__send" onClick={send} disabled={!text.trim()}>
                    ➤
                </button>
            </footer>
        </div>
    );
}

export default App;