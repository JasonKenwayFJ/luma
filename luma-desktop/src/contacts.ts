import { ChatSummary } from "./types";

const KEY = "luma:contacts";

export interface Contact {
    userId: string;
    username: string;
}

export function loadContacts(): Contact[] {
    try {
        const raw = localStorage.getItem(KEY);
        if (!raw) return [];
        const arr = JSON.parse(raw);
        return Array.isArray(arr) ? arr : [];
    } catch {
        return [];
    }
}

export function addContact(contact: Contact) {
    const existing = loadContacts();
    if (existing.some((c) => c.userId === contact.userId)) return;
    localStorage.setItem(KEY, JSON.stringify([...existing, contact]));
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