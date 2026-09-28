import { UserProfile } from "./types";

const USER_ID_KEY = "luma:userId";
const PROFILE_KEY = "luma:profile";

// userId остаётся тем же, что был раньше, поэтому старые сообщения
// после регистрации по-прежнему считаются твоими.
export function getOrCreateUserId(): string {
    let id = localStorage.getItem(USER_ID_KEY);
    if (!id) {
        id = crypto.randomUUID();
        localStorage.setItem(USER_ID_KEY, id);
    }
    return id;
}

export function loadProfile(): UserProfile | null {
    try {
        const raw = localStorage.getItem(PROFILE_KEY);
        if (!raw) return null;
        const p = JSON.parse(raw);
        if (
            p &&
            typeof p.userId === "string" &&
            typeof p.name === "string" &&
            typeof p.username === "string"
        ) {
            return p as UserProfile;
        }
        return null;
    } catch {
        return null;
    }
}

export function saveProfile(profile: UserProfile) {
    localStorage.setItem(PROFILE_KEY, JSON.stringify(profile));
}

export function clearProfile() {
    localStorage.removeItem(PROFILE_KEY);
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

export function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} Б`;
    if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} КБ`;
    return `${(bytes / 1024 / 1024).toFixed(1)} МБ`;
}

export function avatarColor(seed: string): string {
    let hash = 0;
    for (let i = 0; i < seed.length; i++) {
        hash = seed.charCodeAt(i) + ((hash << 5) - hash);
    }
    return `hsl(${Math.abs(hash) % 360}, 55%, 45%)`;
}