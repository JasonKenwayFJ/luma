// push.ts
import {
    requestPermission,
    registerForPush,
    onNotificationReceived,
    onNotificationTapped
} from "@spicavi/tauri-plugin-push-notifications";

export async function initPush() {
    const allowed = await requestPermission();

    if (!allowed) {
        console.log("Push permission denied");
        return;
    }

    const token = await registerForPush();

    console.log("FCM TOKEN:", token);

    // Отправляем token на твой Axum сервер
    await fetch("https://твой-сервер/device/register", {
        method: "POST",
        headers: {
            "Content-Type": "application/json"
        },
        body: JSON.stringify({
            token
        })
    });

    await onNotificationReceived((notification) => {
        console.log("PUSH:", notification);
    });

    await onNotificationTapped((notification) => {
        console.log("CLICK:", notification);
    });
}