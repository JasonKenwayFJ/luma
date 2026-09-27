// То, что реально летит по WebSocket. own здесь нет намеренно —
// это состояние конкретного клиента, а не свойство сообщения.
export interface WireMessage {
    id: string;
    userId: string;
    roomId: string; // пока всегда "general", задел на комнаты
    authorName: string;
    avatarUrl?: string;
    text: string;
    sentAt: string; // ISO 8601, чтобы сортировалось и парсилось однозначно
}

// То, что хранится в state и рендерится. own считается локально
// при получении/отправке, поэтому расширяем WireMessage, а не дублируем поля.
export interface ChatMessage extends WireMessage {
    own: boolean;
}