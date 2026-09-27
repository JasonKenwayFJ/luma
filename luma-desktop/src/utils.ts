const USER_ID_KEY = "luma:userId";
const USER_NAME_KEY = "luma:userName";

// userId генерируется один раз на инсталляцию приложения и живёт в localStorage
// вкладки/webview. Это НЕ авторизация — просто способ отличить "себя" от других
// до появления настоящего логина.
export function getOrCreateUserId(): string {
    let id = localStorage.getItem(USER_ID_KEY);
    if (!id) {
        id = crypto.randomUUID();
        localStorage.setItem(USER_ID_KEY, id);
    }
    return id;
}

export function getOrCreateUserName(): string {
    let name = localStorage.getItem(USER_NAME_KEY);
    if (!name) {
        name = `Guest_${Math.floor(Math.random() * 9000 + 1000)}`;
        localStorage.setItem(USER_NAME_KEY, name);
    }
    return name;
}

export function setUserName(name: string) {
    localStorage.setItem(USER_NAME_KEY, name);
}

export function initials(name: string): string {
    return name
        .trim()
        .split(/\s+/)
        .slice(0, 2)
        .map((w) => w[0]?.toUpperCase() ?? "")
        .join("");
}

export function formatTime(iso: string): string {
    return new Date(iso).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
    });
}

// Детерминированный цвет аватарки из userId (простой хэш строки в число,
// число — в hue для hsl). Один и тот же человек всегда получает один и тот
// же цвет, без сервера и без хранения этого цвета где-либо.
export function avatarColor(seed: string): string {
    let hash = 0;
    for (let i = 0; i < seed.length; i++) {
        hash = seed.charCodeAt(i) + ((hash << 5) - hash);
    }
    return `hsl(${Math.abs(hash) % 360}, 55%, 45%)`;
}