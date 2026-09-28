import { useEffect, useRef, useState, type ChangeEvent, type KeyboardEvent } from "react";
import { AttachmentMeta } from "../types";
import "./Composer.scss";

interface Props {
    connected: boolean;
    onSend: (text: string, attachments: AttachmentMeta[]) => Promise<void>;
}

const MAX_FILES = 10;

const isVideo = (f: File) => f.type.startsWith("video/");

function Composer({ connected, onSend }: Props) {
    const [text, setText] = useState("");
    const [files, setFiles] = useState<File[]>([]);
    const [previews, setPreviews] = useState<{ file: File; url: string }[]>([]);
    const fileInputRef = useRef<HTMLInputElement>(null);

    // Превью строятся из blob-URL. Каждый URL держит файл в памяти,
    // пока его не освободят, поэтому cleanup-функция эффекта делает
    // revokeObjectURL: она срабатывает перед пересозданием превью
    // (при смене files) и при размонтировании компонента.
    useEffect(() => {
        const next = files.map((file) => ({ file, url: URL.createObjectURL(file) }));
        setPreviews(next);
        return () => next.forEach((p) => URL.revokeObjectURL(p.url));
    }, [files]);

    const handlePick = (e: ChangeEvent<HTMLInputElement>) => {
        // Сначала копируем FileList в массив: он «живой», и сброс value ниже его очистит.
        const picked = Array.from(e.target.files ?? []);
        setFiles((prev) => [...prev, ...picked].slice(0, MAX_FILES));
        // Сброс нужен, чтобы onChange сработал снова, если выбрать тот же файл ещё раз.
        e.target.value = "";
    };

    const removeFile = (index: number) => {
        setFiles((prev) => prev.filter((_, i) => i !== index));
    };

    const canSend = connected && (text.trim().length > 0 || files.length > 0);

    const send = async () => {
        if (!canSend) return;
        const attachments: AttachmentMeta[] = files.map((f) => ({
            name: f.name,
            kind: isVideo(f) ? "video" : "image",
            size: f.size,
        }));
        await onSend(text.trim(), attachments);
        setText("");
        setFiles([]);
    };

    const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
        }
    };

    return (
        <footer className="composer">
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
                >
                    📎
                </button>
                <input
                    className="composer__input"
                    value={text}
                    onChange={(e) => setText(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder={connected ? "Напишите сообщение..." : "Нет связи с сервером..."}
                />
                <button className="composer__send" onClick={send} disabled={!canSend}>
                    ➤
                </button>
            </div>
        </footer>
    );
}

export default Composer;