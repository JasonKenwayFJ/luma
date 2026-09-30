import { useEffect, useRef, useState, type ChangeEvent, type KeyboardEvent } from "react";
import { AttachmentMeta } from "../types";
import "./Composer.scss";

interface Props {
    connected: boolean;
    onSend: (text: string, attachments: AttachmentMeta[]) => Promise<void>;
}

const MAX_FILES = 5;
const MAX_FILE_BYTES = 5 * 1024 * 1024; // 5 МБ на файл

const isVideo = (f: File) => f.type.startsWith("video/");

// FileReader — коллбэк-based API из старого браузерного мира, промис
// оборачивает его в await-совместимую форму. onload и onerror сработают
// ровно один раз каждый, поэтому reject/resolve не конфликтуют.
function readAsBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => {
            const result = reader.result as string;
            // readAsDataURL отдаёт "data:image/png;base64,iVBORw0KG...".
            // На сервер и в базу летит только то, что после запятой.
            const base64 = result.slice(result.indexOf(",") + 1);
            resolve(base64);
        };
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
    });
}

function Composer({ connected, onSend }: Props) {
    const [text, setText] = useState("");
    const [files, setFiles] = useState<File[]>([]);
    const [previews, setPreviews] = useState<{ file: File; url: string }[]>([]);
    const [error, setError] = useState<string | null>(null);
    const [sending, setSending] = useState(false);
    const fileInputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        const next = files.map((file) => ({ file, url: URL.createObjectURL(file) }));
        setPreviews(next);
        return () => next.forEach((p) => URL.revokeObjectURL(p.url));
    }, [files]);

    const handlePick = (e: ChangeEvent<HTMLInputElement>) => {
        const picked = Array.from(e.target.files ?? []);
        e.target.value = "";

        const tooBig = picked.filter((f) => f.size > MAX_FILE_BYTES);
        const ok = picked.filter((f) => f.size <= MAX_FILE_BYTES);

        if (tooBig.length > 0) {
            setError(`Слишком большой файл (макс. 5 МБ): ${tooBig.map((f) => f.name).join(", ")}`);
        } else {
            setError(null);
        }

        setFiles((prev) => [...prev, ...ok].slice(0, MAX_FILES));
    };

    const removeFile = (index: number) => {
        setFiles((prev) => prev.filter((_, i) => i !== index));
    };

    const canSend = connected && !sending && (text.trim().length > 0 || files.length > 0);

    const send = async () => {
        if (!canSend) return;
        setSending(true);
        setError(null);
        try {
            // Файлы читаются параллельно: Promise.all ждёт все FileReader'ы
            // разом, вместо того чтобы кодировать их по очереди.
            const attachments: AttachmentMeta[] = await Promise.all(
                files.map(async (f) => ({
                    name: f.name,
                    kind: isVideo(f) ? ("video" as const) : ("image" as const),
                    size: f.size,
                    mimeType: f.type || "application/octet-stream",
                    dataBase64: await readAsBase64(f),
                }))
            );
            await onSend(text.trim(), attachments);
            setText("");
            setFiles([]);
        } catch {
            setError("Не удалось прочитать файл, попробуйте ещё раз");
        } finally {
            setSending(false);
        }
    };

    const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
        }
    };

    return (
        <footer className="composer">
            {error && <div className="composer__error">{error}</div>}

            {previews.length > 0 && (
                <div className="composer__attachments">
                    {previews.map((p, i) => (
                        <div className="attach" key={`${p.file.name}-${i}`}>
                            {isVideo(p.file) ? (
                                <video src={p.url} preload="metadata" muted />
                            ) : (
                                <img src={p.url} alt={p.file.name} />
                            )}
                            <button
                                className="attach__remove"
                                onClick={() => removeFile(i)}
                                title="Убрать"
                                disabled={sending}
                            >
                                ×
                            </button>
                            <span className="attach__name">{p.file.name}</span>
                        </div>
                    ))}
                </div>
            )}

            <div className="composer__row">
                <input
                    ref={fileInputRef}
                    type="file"
                    accept="image/*,video/*"
                    multiple
                    hidden
                    onChange={handlePick}
                />
                <button
                    className="composer__attach"
                    onClick={() => fileInputRef.current?.click()}
                    title="Прикрепить фото или видео"
                    disabled={sending}
                >
                    📎
                </button>
                <input
                    className="composer__input"
                    value={text}
                    onChange={(e) => setText(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder={
                        connected
                            ? sending
                                ? "Отправка вложений..."
                                : "Напишите сообщение..."
                            : "Нет связи с сервером..."
                    }
                    disabled={sending}
                />
                <button className="composer__send" onClick={send} disabled={!canSend}>
                    {sending ? "…" : "➤"}
                </button>
            </div>
        </footer>
    );
}

export default Composer;