import {useState, type ChangeEvent, type FormEvent} from "react";
import "./RegisterScreen.scss";
import icon from "../assets/luma-icon.png";

interface Props {
    onSubmit: (data: { name: string; username: string }) => void;
}

const USERNAME_RE = /^[a-z0-9_]{3,20}$/;

function RegisterScreen({onSubmit}: Props) {
    const [name, setName] = useState("");
    const [username, setUsername] = useState("");
    const [touched, setTouched] = useState(false);

    const trimmedName = name.trim();
    const nameError = trimmedName.length < 2 ? "Минимум 2 символа" : null;
    const usernameError = USERNAME_RE.test(username)
        ? null
        : "3–20 символов: a–z, 0–9, _";

    const handleUsername = (e: ChangeEvent<HTMLInputElement>) => {
        // Лишние символы отсекаются прямо при вводе.
        setUsername(e.target.value.toLowerCase().replace(/[^a-z0-9_]/g, ""));
    };

    const handleSubmit = (e: FormEvent) => {
        // Без preventDefault браузер отправил бы форму и перезагрузил страницу.
        e.preventDefault();
        setTouched(true);
        if (nameError || usernameError) return;
        onSubmit({name: trimmedName, username});
    };

    return (
        <div className="register">
            <form className="register__card" onSubmit={handleSubmit}>
                <div className="register__logo">
                    <img src={icon} alt="Luma"/>
                </div>

                <h1 className="register__title">Добро пожаловать в Luma</h1>
                <p className="register__subtitle">Создайте профиль, чтобы начать общение</p>

                <label className="field">
                    <span className="field__label">Имя</span>
                    <input
                        className={`field__input ${touched && nameError ? "field__input--error" : ""}`}
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        maxLength={20}
                        placeholder="Как вас называть"
                        autoFocus
                    />
                    {touched && nameError && <span className="field__error">{nameError}</span>}
                </label>

                <label className="field">
                    <span className="field__label">Логин</span>
                    <input
                        className={`field__input ${touched && usernameError ? "field__input--error" : ""}`}
                        value={username}
                        onChange={handleUsername}
                        maxLength={20}
                        placeholder="username"
                    />
                    {touched && usernameError && (
                        <span className="field__error">{usernameError}</span>
                    )}
                </label>

                <button className="register__submit" type="submit">
                    Продолжить
                </button>

                <p className="register__note">
                    Профиль хранится на этом устройстве. Позже здесь появится вход по паролю.
                </p>
            </form>
        </div>
    );
}

export default RegisterScreen;