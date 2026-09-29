export interface AttachmentMeta {
    name: string;
    kind: "image" | "video";
    size: number;
}

// То, что реально летит по WebSocket.
export interface WireMessage {
    id: string;
    userId: string;
    roomId: string;
    authorName: string;
    avatarUrl?: string;
    text: string;
    attachments?: AttachmentMeta[]; // пока только метаданные, сами файлы не передаются
    sentAt: string;
}

// То, что хранится в state: own считается локально.
export interface ChatMessage extends WireMessage {
    own: boolean;
}

export interface UserProfile {
    userId: string;
    name: string;
    username: string;
    createdAt: string;
}

export interface ChatSummary {
    id: string; // совпадает с roomId в сообщениях
    title: string;
}
export type ServerFrame =
    | { type: "summary"; messages: WireMessage[] }
    | { type: "history"; roomId: string; messages: WireMessage[] }
    | (WireMessage & { type: "message" });