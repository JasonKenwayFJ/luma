import { ChatMessage } from "../types";
import { initials, formatTime, avatarColor } from "../utils";
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
                        {attachments.map((a, i) => {
                            const src = `data:${a.mimeType};base64,${a.dataBase64}`;
                            return (
                                <div className="media" key={i}>
                                    {a.kind === "video" ? (
                                        <video src={src} controls preload="metadata" />
                                    ) : (
                                        <img src={src} alt={a.name} />
                                    )}
                                </div>
                            );
                        })}
                    </div>
                )}

                {text && <div className="bubble__text">{text}</div>}
                <div className="bubble__time">{formatTime(sentAt)}</div>
            </div>
        </div>
    );
}

export default Message;