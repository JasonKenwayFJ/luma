import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChatMessage, ChatSummary, UserProfile } from "../types";
import { avatarColor, formatTime, initials } from "../utils";
import ConnectionStatus from "./ConnectionStatus";
import "./ChatList.scss";

interface SearchResult {
    userId: string;
    username: string;
}

interface Props {
    profile: UserProfile;
    chats: ChatSummary[];
    messages: ChatMessage[];
    connected: boolean;
    onOpen: (chatId: string) => void;
    onStartDirect: (userId: string, username: string) => void;
    onLogout: () => void;
    onSettings: () => void;
}

function lastMessageOf(messages: ChatMessage[], roomId: string): ChatMessage | undefined {
    for (let i = messages.length - 1; i >= 0; i--) {
        if (messages[i].roomId === roomId) return messages[i];
    }
    return undefined;
}

function previewOf(m: ChatMessage): string {
    const who = m.own ? "Вы" : m.authorName;
    const body = m.text || "📎 Вложение";
    return `${who}: ${body}`;
}

function ChatList({ profile, chats, messages, connected, onOpen, onStartDirect, onLogout, onSettings }: Props) {
    const [searchOpen, setSearchOpen] = useState(false);
    const [query, setQuery] = useState("");
    const [results, setResults] = useState<SearchResult[]>([]);
    const [searching, setSearching] = useState(false);
    const [searchError, setSearchError] = useState("");
    const requestId = useRef(0);
    const normalizedQuery = query.trim().replace(/^@/, "");

    // Debounce: таймер сбрасывается на каждое нажатие клавиши, и запрос
    // на сервер уходит только если пользователь замер на 300мс — иначе
    // при быстром наборе слова ушло бы по запросу на каждую букву.
    useEffect(() => {
        const currentRequest = ++requestId.current;
        if (!searchOpen || normalizedQuery.length < 2) {
            setResults([]);
            setSearching(false);
            setSearchError("");
            return;
        }
        const handle = setTimeout(async () => {
            setSearching(true);
            setSearchError("");
            setResults([]);
            try {
                const found = await invoke<SearchResult[]>("search_users", {
                    query: normalizedQuery,
                    token: profile.token,
                });
                if (requestId.current === currentRequest) setResults(found);
            } catch (error) {
                if (requestId.current === currentRequest) {
                    setResults([]);
                    setSearchError(error instanceof Error ? error.message : "Не удалось выполнить поиск.");
                }
            } finally {
                if (requestId.current === currentRequest) setSearching(false);
            }
        }, 300);
        return () => clearTimeout(handle);
    }, [normalizedQuery, searchOpen, profile.token]);

    const handleStartDirect = (r: SearchResult) => {
        onStartDirect(r.userId, r.username);
        setSearchOpen(false);
        setQuery("");
        setResults([]);
    };

    return (
        <div className="screen">
            <header className="topbar">
                <div className="topbar__avatar" style={{ background: avatarColor(profile.userId) }}>
                    {initials(profile.username)}
                </div>
                <div className="topbar__title">{profile.username}</div>
                <div className="topbar__spacer" />
                <ConnectionStatus connected={connected} />
                <button className="icon-btn" onClick={() => setSearchOpen((v) => !v)} title="Найти человека">
                    🔍
                </button>
                <button className="icon-btn" onClick={onSettings} title="Настройки" aria-label="Настройки">
                    ⚙
                </button>
                <button className="link-btn" onClick={onLogout}>
                    Выйти
                </button>
            </header>

            {searchOpen && (
                <div className="search-panel">
                    <input
                        className="search-panel__input"
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        placeholder="Поиск по username, например @anna"
                        autoFocus
                    />
                    {searching && <div className="search-panel__hint">Ищем...</div>}
                    {searchError && <div className="search-panel__hint search-panel__hint--error">{searchError}</div>}
                    {!searching && !searchError && normalizedQuery.length >= 2 && results.length === 0 && (
                        <div className="search-panel__hint">Никого не нашли</div>
                    )}
                    {normalizedQuery.length < 2 && <div className="search-panel__hint">Введите минимум 2 символа username.</div>}
                    {results.map((r) => (
                        <button key={r.userId} className="search-panel__result" onClick={() => handleStartDirect(r)}>
                            <div className="search-panel__avatar" style={{ background: avatarColor(r.userId) }}>
                                {initials(r.username)}
                            </div>
                            <span>@{r.username}</span>
                            <span className="search-panel__action">Написать</span>
                        </button>
                    ))}
                </div>
            )}

            <main className="chatlist">
                {chats.map((chat, index) => {
                    const last = lastMessageOf(messages, chat.id);
                    return (
                        <div key={chat.id}>
                        {index === 0 && <div className="chatlist__section">Чаты</div>}
                        {chat.id.startsWith("dm:") && !chats[index - 1]?.id.startsWith("dm:") && <div className="chatlist__section">Личные чаты</div>}
                        <button className="chat-row" onClick={() => onOpen(chat.id)}>
                            <div className="chat-row__avatar" style={{ background: avatarColor(chat.id) }}>
                                {chat.title[0]}
                            </div>
                            <div className="chat-row__body">
                                <div className="chat-row__top">
                                    <span className="chat-row__title">{chat.title}</span>
                                    {last && <span className="chat-row__time">{formatTime(last.sentAt)}</span>}
                                </div>
                                <div className="chat-row__preview">{last ? previewOf(last) : "Нет сообщений"}</div>
                            </div>
                        </button>
                        </div>
                    );
                })}
                {chats.length === 0 && <div className="chatlist__empty">Пока нет чатов. Найдите человека по username, чтобы начать переписку.</div>}
            </main>
        </div>
    );
}

export default ChatList;
