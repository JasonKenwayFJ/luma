import { useState, type ReactNode } from "react";
import { useChat } from "./hooks/useChat";
import { CHATS } from "./chats";
import { UserProfile } from "./types";
import { clearProfile, getOrCreateUserId, loadProfile, saveProfile } from "./utils";
import RegisterScreen from "./components/RegisterScreen";
import ChatList from "./components/ChatList";
import ChatView from "./components/ChatView";
import "./App.scss";

function App() {
    // Функция без вызова = ленивая инициализация: читается localStorage
    // один раз при первом рендере, а не на каждом.
    const [userId] = useState(getOrCreateUserId);
    const [profile, setProfile] = useState<UserProfile | null>(loadProfile);
    const [activeChatId, setActiveChatId] = useState<string | null>(null);

    const { messages, connected, send } = useChat(userId);

    const handleRegister = (data: { name: string; username: string }) => {
        const p: UserProfile = {
            userId,
            ...data,
            createdAt: new Date().toISOString(),
        };
        // Здесь позже будет запрос регистрации на сервер.
        saveProfile(p);
        setProfile(p);
    };

    const handleLogout = () => {
        clearProfile();
        setProfile(null);
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
                onBack={() => setActiveChatId(null)}
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