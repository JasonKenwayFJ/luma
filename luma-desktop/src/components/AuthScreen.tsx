import { useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { UserProfile } from "../types";
import "./AuthScreen.scss";

interface Props {
    onAuthenticated: (profile: UserProfile) => void;
}

function isValidUsername(u: string) {
    return /^[a-z0-9_]{3,20}$/.test(u);
}

function AuthScreen({ onAuthenticated }: Props) {
    const [mode, setMode] = useState<"login" | "register">("login");
    const [email, setEmail] = useState("");
    const [password, setPassword] = useState("");
    const [username, setUsername] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);

    const handleSubmit = async (e: FormEvent) => {
        e.preventDefault();
        setError(null);

        const trimmedEmail = email.trim().toLowerCase();
        if (!trimmedEmail.includes("@")) {
            setError("Введите корректный email");
            return;
        }
        if (password.length < 6) {
            setError("Пароль минимум 6 символов");
            return;
        }
        if (mode === "register" && !isValidUsername(username)) {
            setError("Юзернейм: 3–20 символов, a–z, 0–9, _");
            return;
        }

        setBusy(true);
        try {
            const result =
                mode === "login"
                    ? await invoke<UserProfile>("login", { email: trimmedEmail, password })
                    : await invoke<UserProfile>("register", {
                        email: trimmedEmail,
                        password,
                        username: username.trim().toLowerCase(),
                    });

            onAuthenticated(result);
        } catch (err) {
            setError(typeof err === "string" ? err : "Что-то пошло не так");
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="auth">
            <form className="auth__card" onSubmit={handleSubmit}>
                <div className="auth__logo">L</div>
                <h1 className="auth__title">
                    {mode === "login" ? "С возвращением" : "Добро пожаловать в Luma"}
                </h1>

                <div className="auth__tabs">
                    <button
                        type="button"
                        className={`auth__tab ${mode === "login" ? "auth__tab--active" : ""}`}
                        onClick={() => setMode("login")}
                    >
                        Войти
                    </button>
                    <button
                        type="button"
                        className={`auth__tab ${mode === "register" ? "auth__tab--active" : ""}`}
                        onClick={() => setMode("register")}
                    >
                        Регистрация
                    </button>
                </div>

                <label className="field">
                    <span className="field__label">Email</span>
                    <input
                        className="field__input"
                        type="email"
                        value={email}
                        onChange={(e) => setEmail(e.target.value)}
                        placeholder="you@example.com"
                        autoFocus
                    />
                </label>

                {mode === "register" && (
                    <label className="field">
                        <span className="field__label">Юзернейм</span>
                        <input
                            className="field__input"
                            value={username}
                            onChange={(e) => setUsername(e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, ""))}
                            maxLength={20}
                            placeholder="username"
                        />
                    </label>
                )}

                <label className="field">
                    <span className="field__label">Пароль</span>
                    <input
                        className="field__input"
                        type="password"
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        placeholder="••••••••"
                    />
                </label>

                {error && <div className="field__error">{error}</div>}

                <button className="auth__submit" type="submit" disabled={busy}>
                    {busy ? "Подождите..." : mode === "login" ? "Войти" : "Создать аккаунт"}
                </button>
            </form>
        </div>
    );
}

export default AuthScreen;