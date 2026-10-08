import { useCallback, useEffect, useRef, useState } from "react";
import type { RTCSignal } from "./useChat";

const RTC_CONFIGURATION: RTCConfiguration = {
    iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
};

export function useCall(roomId: string, userId: string, onSignal: (handler: (frame: { from: string; roomId: string; signal: RTCSignal }) => void) => () => void, sendSignal: (to: string, roomId: string, signal: RTCSignal) => void) {
    const peer = useRef<RTCPeerConnection | null>(null);
    const local = useRef<MediaStream | null>(null);
    const screen = useRef<MediaStream | null>(null);
    const [incoming, setIncoming] = useState<string | null>(null);
    const [active, setActive] = useState(false);
    const [localStream, setLocalStream] = useState<MediaStream | null>(null);
    const [remoteStream, setRemoteStream] = useState<MediaStream | null>(null);
    const [error, setError] = useState("");
    const pendingIce = useRef<RTCIceCandidateInit[]>([]);

    const close = useCallback(() => {
        peer.current?.close();
        peer.current = null;
        pendingIce.current = [];
        local.current?.getTracks().forEach((track) => track.stop());
        screen.current?.getTracks().forEach((track) => track.stop());
        local.current = null;
        screen.current = null;
        setLocalStream(null);
        setRemoteStream(null);
        setActive(false);
        setIncoming(null);
    }, []);

    const createPeer = useCallback((otherId: string) => {
        const connection = new RTCPeerConnection(RTC_CONFIGURATION);
        peer.current = connection;
        // Reserve a video sender up front so screen sharing needs no renegotiation.
        connection.addTransceiver("video", { direction: "sendrecv" });
        local.current?.getTracks().forEach((track) => connection.addTrack(track, local.current!));
        connection.onicecandidate = (event) => {
            if (event.candidate) sendSignal(otherId, roomId, { kind: "ice", candidate: event.candidate.toJSON() });
        };
        connection.ontrack = (event) => setRemoteStream(event.streams[0] ?? new MediaStream([event.track]));
        connection.onconnectionstatechange = () => {
            if (connection.connectionState === "failed" || connection.connectionState === "disconnected") {
                setError("Связь прервалась. Попробуйте позвонить ещё раз.");
            }
        };
        return connection;
    }, [roomId, sendSignal]);

    const start = useCallback(async (video = false) => {
        const otherId = roomId.startsWith("dm:") ? roomId.slice(3).split(":").find((id) => id !== userId) ?? "" : "";
        if (!otherId || !roomId.startsWith("dm:")) return setError("Звонки доступны в личных чатах.");
        try {
            setError("");
            pendingIce.current = [];
            local.current = await navigator.mediaDevices.getUserMedia({ audio: true, video });
            setLocalStream(local.current);
            const connection = createPeer(otherId);
            const offer = await connection.createOffer();
            await connection.setLocalDescription(offer);
            sendSignal(otherId, roomId, { kind: "offer", description: offer });
            setActive(true);
        } catch (cause) {
            close();
            setError(cause instanceof Error ? cause.message : "Не удалось начать звонок.");
        }
    }, [close, createPeer, roomId, sendSignal, userId]);

    useEffect(() => onSignal(async ({ from, roomId: signalRoom, signal }) => {
        if (signalRoom !== roomId) return;
        if (signal.kind === "hangup") { close(); return; }
        if (signal.kind === "offer") {
            setIncoming(from);
            peer.current?.close();
            peer.current = null;
            pendingIce.current = [];
            // Keep the offer until the user accepts the call.
            pendingOffer.current = signal.description;
        } else if (signal.kind === "answer" && peer.current) {
            await peer.current.setRemoteDescription(signal.description);
            await Promise.all(pendingIce.current.splice(0).map((candidate) => peer.current!.addIceCandidate(candidate).catch(() => undefined)));
        } else if (signal.kind === "ice") {
            if (peer.current?.remoteDescription) await peer.current.addIceCandidate(signal.candidate).catch(() => undefined);
            else pendingIce.current.push(signal.candidate);
        }
    }), [close, onSignal, roomId]);

    const pendingOffer = useRef<RTCSessionDescriptionInit | null>(null);
    const answer = useCallback(async () => {
        if (!incoming || !pendingOffer.current) return;
        try {
            local.current = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
            setLocalStream(local.current);
            const connection = createPeer(incoming);
            await connection.setRemoteDescription(pendingOffer.current);
            await Promise.all(pendingIce.current.splice(0).map((candidate) => connection.addIceCandidate(candidate).catch(() => undefined)));
            const reply = await connection.createAnswer();
            await connection.setLocalDescription(reply);
            sendSignal(incoming, roomId, { kind: "answer", description: reply });
            pendingOffer.current = null;
            setActive(true);
        } catch (cause) { setError(cause instanceof Error ? cause.message : "Не удалось ответить на звонок."); }
    }, [createPeer, incoming, roomId, sendSignal]);

    const shareScreen = useCallback(async () => {
        try {
            const stream = await navigator.mediaDevices.getDisplayMedia({ video: true });
            screen.current = stream;
            const track = stream.getVideoTracks()[0];
            const sender = peer.current?.getSenders().find((item) => item.track?.kind === "video");
            if (sender) await sender.replaceTrack(track);
            else throw new Error("Не удалось подготовить видеоканал.");
            track.onended = () => { screen.current = null; };
            setLocalStream(stream);
        } catch (cause) { setError(cause instanceof Error ? cause.message : "Не удалось включить трансляцию."); }
    }, []);

    const hangup = useCallback(() => {
        const other = incoming ?? (roomId.startsWith("dm:") ? roomId.slice(3).split(":").find((id) => id !== userId) : undefined);
        if (other) sendSignal(other, roomId, { kind: "hangup" });
        close();
    }, [close, incoming, roomId, sendSignal, userId]);

    return { incoming, active, localStream, remoteStream, error, start, answer, shareScreen, hangup };
}
