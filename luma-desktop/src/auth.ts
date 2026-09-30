import { UserProfile } from "./types";

const KEY = "luma:auth";

export function loadProfile(): UserProfile | null {
    try {
        const raw = localStorage.getItem(KEY);
        if (!raw) return null;
        const p = JSON.parse(raw);
        if (p && typeof p.token === "string" && typeof p.userId === "string" && typeof p.username === "string") {
            return p as UserProfile;
        }
        return null;
    } catch {
        return null;
    }
}

export function saveProfile(profile: UserProfile) {
    localStorage.setItem(KEY, JSON.stringify(profile));
}

export function clearProfile() {
    localStorage.removeItem(KEY);
}