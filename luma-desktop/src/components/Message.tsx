import { ChatMessage } from "../types";
import { initials, formatTime, formatSize, avatarColor } from "../utils";
import "./Message.scss";

interface MessageProps {
    message: ChatMessage;
}

function Message({ message }: MessageProps) {
    const { own, authorName, avatarUrl, text, attachments, sentAt, userId } = message;

    return (
        <div className={`row ${own ? "row--own" : "row--other"}`}>
            {!own && (
                <div
                    className="avatar"
                    style={{ background: avatarColor(userId) }}
                    title={authorName}
                >
                    {avatarUrl ? <img src={avatarUrl} alt={authorName} /> : initials(authorName)}
                </div>
            )}
            <div className={`bubble ${own ? "bubble--own" : "bubble--other"}`}>
                {!own && <div className="bubble__author">{authorName}</div>}

                {attachments && attachments.length > 0 && (
                    <div className="bubble__attachments">
                        {attachments.map((a, i) => (
                            <div className="chip" key={i}>
                                <span>{a.kind === "video" ? "🎬" : "🖼"}</span>
                                <span className="chip__name">{a.name}</span>
                                <span className="chip__size">{formatSize(a.size)}</span>
                            </div>
                        ))}
                    </div>
                )}

                {text && <div className="bubble__text">{text}</div>}
                <div className="bubble__time">{formatTime(sentAt)}</div>
            </div>
        </div>
    );
}

export default Message;