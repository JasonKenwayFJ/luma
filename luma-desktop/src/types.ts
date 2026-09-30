export interface AttachmentMeta {
    name: string;
    kind: "image" | "video";
    size: number;
    mimeType: string;
    dataBase64: string;
}

export interface WireMessage {
    id: string;
    userId: string;
    roomId: string;
    authorName: string;
    avatarUrl?: string;
    text: string;
    attachments?: AttachmentMeta[];
    sentAt: string;
}

export interface ChatMessage extends WireMessage {
    own: boolean;
}

export interface UserProfile {
    token: string;
    userId: string;
    username: string;
}

export interface ChatSummary {
    id: string;
    title: string;
}

export type ServerFrame =
    | { type: "summary"; messages: WireMessage[] }
    | { type: "history"; roomId: string; messages: WireMessage[]; hasMore: boolean; isInitial: boolean }
    | (WireMessage & { type: "message" })
    | (WireMessage & { type: "preview" });