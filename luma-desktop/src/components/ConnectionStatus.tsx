function ConnectionStatus({ connected }: { connected: boolean }) {
    return (
        <span className={`status ${connected ? "status--online" : "status--offline"}`}>
      <span className="status__dot" />
            {connected ? "в сети" : "нет связи"}
    </span>
    );
}

export default ConnectionStatus;