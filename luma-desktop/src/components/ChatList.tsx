import { ChatMessage, ChatSummary, UserProfile } from "../types";
import { avatarColor, formatTime, initials } from "../utils";
import ConnectionStatus from "./ConnectionStatus";
import "./ChatList.scss";

interface Props {
    profile: UserProfile;
    chats: ChatSummary[];
    messages: ChatMessage[];
    connected: boolean;
    onOpen: (chatId: string) => void;
    onLogout: () => void;
}

// messages отсортирован по времени по возрастанию, поэтому идём с конца:
// первое совпадение по roomId и есть последнее сообщение чата.
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

function ChatList({ profile, chats, messages, connected, onOpen, onLogout }: Props) {
    return (
        <div className="screen">
            <header className="topbar">
                <div
                    className="topbar__avatar"
                    style={{ background: avatarColor(profile.userId) }}
                >
                    {initials(profile.name)}
                </div>
                <div>
                    <div className="topbar__title">{profile.name}</div>
                    <div className="topbar__sub">@{profile.username}</div>
                </div>
                <div className="topbar__spacer" />
                <ConnectionStatus connected={connected} />
                <button className="link-btn" onClick={onLogout}>
                    Сменить профиль
                </button>
            </header>

            <main className="chatlist">
                {chats.map((chat) => {
                    const last = lastMessageOf(messages, chat.id);
                    return (
                        <button key={chat.id} className="chat-row" onClick={() => onOpen(chat.id)}>
                            <div
                                className="chat-row__avatar"
                                style={{ background: avatarColor(chat.id) }}
                            >
                                {chat.title[0]}
                            </div>
                            <div className="chat-row__body">
                                <div className="chat-row__top">
                                    <span className="chat-row__title">{chat.title}</span>
                                    {last && (
                                        <span className="chat-row__time">{formatTime(last.sentAt)}</span>
                                    )}
                                </div>
                                <div className="chat-row__preview">
                                    {last ? previewOf(last) : "Нет сообщений"}
                                </div>
                            </div>
                        </button>
                    );
                })}
            </main>
        </div>
    );
}

export default ChatList;