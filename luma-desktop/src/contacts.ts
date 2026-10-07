import { ChatSummary } from "./types";

const KEY_PREFIX = "luma:contacts:";
const LEGACY_KEY = "luma:contacts";

export interface Contact {
    userId: string;
    username: string;
}

export function loadContacts(userId: string): Contact[] {
    if (!userId) return [];
    try {
        const key = `${KEY_PREFIX}${userId}`;
        let raw = localStorage.getItem(key);
        if (!raw) {
            // Keep contacts saved by older versions and associate them with
            // the account that is currently signed in.
            raw = localStorage.getItem(LEGACY_KEY);
            if (raw) {
                localStorage.setItem(key, raw);
                localStorage.removeItem(LEGACY_KEY);
            }
        }
        if (!raw) return [];
        const arr = JSON.parse(raw);
        return Array.isArray(arr)
            ? arr.filter((item): item is Contact =>
                item && typeof item.userId === "string" && typeof item.username === "string")
            : [];
    } catch {
        return [];
    }
}

export function addContact(ownerId: string, contact: Contact) {
    if (!ownerId || !contact.userId || !contact.username) return;
    const existing = loadContacts(ownerId);
    if (existing.some((c) => c.userId === contact.userId)) return;
    localStorage.setItem(`${KEY_PREFIX}${ownerId}`, JSON.stringify([...existing, contact]));
}

// Личный чат не хранится на сервере как отдельная сущность — это просто
// комната с предсказуемым id, который оба участника могут вычислить сами,
// зная только id друг друга. Сортировка гарантирует, что порядок, в
// котором два человека "нашли" друг друга, не важен — у А и Б получится
// одна и та же комната.
export function dmRoomId(a: string, b: string): string {
    return `dm:${[a, b].sort().join(":")}`;
}

export function contactsToChats(contacts: Contact[], myUserId: string): ChatSummary[] {
    return contacts.map((c) => ({ id: dmRoomId(myUserId, c.userId), title: c.username }));
}
