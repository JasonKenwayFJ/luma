import { useEffect, useState, type ReactNode } from "react";
import { useChat } from "./hooks/useChat";
import { CHATS } from "./chats";
import { UserProfile } from "./types";
import { clearProfile, getOrCreateUserId, loadProfile, saveProfile } from "./utils";
import RegisterScreen from "./components/RegisterScreen";
import ChatList from "./components/ChatList";
import ChatView from "./components/ChatView";
import "./App.scss";

function App() {
    const [userId] = useState(getOrCreateUserId);
    const [profile, setProfile] = useState<UserProfile | null>(loadProfile);
    const [activeChatId, setActiveChatId] = useState<string | null>(null);

    const { messages, connected, send, joinRoom, leaveRoom } = useChat(userId);

    // Один эффект покрывает два случая сразу: открытие чата (activeChatId
    // меняется на конкретный id) и восстановление соединения, пока чат уже
    // открыт (connected переключается false → true) — сервер забывает,
    // в какой комнате было соединение, каждый раз при новом подключении.
    useEffect(() => {
        if (connected && activeChatId) {
            joinRoom(activeChatId);
        }
    }, [connected, activeChatId, joinRoom]);

    const handleRegister = (data: { name: string; username: string }) => {
        const p: UserProfile = {
            userId,
            ...data,
            createdAt: new Date().toISOString(),
        };
        saveProfile(p);
        setProfile(p);
    };

    const handleLogout = () => {
        clearProfile();
        setProfile(null);
        setActiveChatId(null);
    };

    const closeChat = () => {
        leaveRoom();
        setActiveChatId(null);
    };

    const activeChat = CHATS.find((c) => c.id === activeChatId);

    let screen: ReactNode;
    if (!profile) {
        screen = <RegisterScreen onSubmit={handleRegister} />;
    } else if (activeChat) {
        screen = (
            <ChatView
                chat={activeChat}
                messages={messages.filter((m) => m.roomId === activeChat.id)}
                connected={connected}
                onBack={closeChat}
                onSend={(text, attachments) =>
                    send({
                        roomId: activeChat.id,
                        authorName: profile.name,
                        text,
                        attachments,
                    })
                }
            />
        );
    } else {
        screen = (
            <ChatList
                profile={profile}
                chats={CHATS}
                messages={messages}
                connected={connected}
                onOpen={setActiveChatId}
                onLogout={handleLogout}
            />
        );
    }

    return <div className="app">{screen}</div>;
}

export default App;