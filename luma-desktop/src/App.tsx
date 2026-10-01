import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import {
    getToken as getFcmToken,
    onNotificationReceived,
    onTokenRefresh,
    requestPermission as requestPushPermission,
} from "tauri-plugin-remote-push-api";
import { useChat } from "./hooks/useChat";
import { CHATS } from "./chats";
import { UserProfile } from "./types";
import { clearProfile, loadProfile, saveProfile } from "./auth";
import { addContact, contactsToChats, dmRoomId, loadContacts, type Contact } from "./contacts";
import AuthScreen from "./components/AuthScreen";
import ChatList from "./components/ChatList";
import ChatView from "./components/ChatView";
import "./App.scss";

function App() {
    const [profile, setProfile] = useState<UserProfile | null>(loadProfile);
    const [contacts, setContacts] = useState<Contact[]>(loadContacts);
    const [activeChatId, setActiveChatId] = useState<string | null>(null);

    const { messages, connected, hasMoreByRoom, loadingMore, send, joinRoom, leaveRoom, loadMore } =
        useChat(profile);

    const directChats = profile ? contactsToChats(contacts, profile.userId) : [];
    const allChats = [...CHATS, ...directChats];
    const activeChat = allChats.find((c) => c.id === activeChatId);

    useEffect(() => {
        if (connected && activeChatId) {
            joinRoom(activeChatId);
        }
    }, [connected, activeChatId, joinRoom]);

    useEffect(() => {
        if (!profile) return;

        let cancelled = false;
        let stopListening: (() => void) | undefined;

        (async () => {
            try {
                if (!(await isPermissionGranted())) {
                    await requestPermission();
                }

                // The remote-push plugin only provides FCM tokens on Android.
                if (!/android/i.test(navigator.userAgent)) return;

                const permission = await requestPushPermission();
                if (!permission.granted) return;

                const registerToken = async (token: string) => {
                    if (!cancelled && token) {
                        await invoke("save_fcm_token", { token, authToken: profile.token });
                    }
                };

                const token = await getFcmToken();
                await registerToken(token);
                const refreshListener = await onTokenRefresh(registerToken);
                const notificationListener = await onNotificationReceived((notification) => {
                    if (cancelled || (!notification.title && !notification.body)) return;
                    sendNotification({
                        title: notification.title || "Luma",
                        body: notification.body || "Новое сообщение",
                    });
                });
                const cleanup = () => {
                    void refreshListener.unregister();
                    void notificationListener.unregister();
                };
                if (cancelled) cleanup();
                else stopListening = cleanup;
            } catch (error) {
                console.warn("Notification permission setup failed:", error);
            }
        })();

        return () => {
            cancelled = true;
            stopListening?.();
        };
    }, [profile]);

    const handleAuthenticated = (p: UserProfile) => {
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

    const handleStartDirect = (userId: string, username: string) => {
        if (!profile) return;
        addContact({ userId, username });
        setContacts(loadContacts());
        setActiveChatId(dmRoomId(profile.userId, userId));
    };

    let screen: ReactNode;
    if (!profile) {
        screen = <AuthScreen onAuthenticated={handleAuthenticated} />;
    } else if (activeChat) {
        screen = (
            <ChatView
                chat={activeChat}
                messages={messages.filter((m) => m.roomId === activeChat.id)}
                connected={connected}
                hasMore={hasMoreByRoom[activeChat.id] ?? false}
                loadingMore={loadingMore}
                onBack={closeChat}
                onSend={(text, attachments) => send({ roomId: activeChat.id, text, attachments })}
                onLoadMore={(oldestSentAt) => loadMore(activeChat.id, oldestSentAt)}
            />
        );
    } else {
        screen = (
            <ChatList
                profile={profile}
                chats={allChats}
                messages={messages}
                connected={connected}
                onOpen={setActiveChatId}
                onStartDirect={handleStartDirect}
                onLogout={handleLogout}
            />
        );
    }

    return <div className="app">{screen}</div>;
}

export default App;
