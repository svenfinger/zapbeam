import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import "./App.css";

type Status = "connecting" | "ready" | "error";

function App() {
  const [status, setStatus] = useState<Status>("connecting");

  useEffect(() => {
    let active = true;
    const timeout = window.setTimeout(() => {
      active = false;
      setStatus("error");
    }, 10000);

    invoke<string>("backend_status")
      .then((result) => {
        if (active) setStatus(result === "ready" ? "ready" : "error");
      })
      .catch((error: unknown) => {
        console.error("Zapbeam backend initialization failed:", error);
        if (active) setStatus("error");
      })
      .finally(() => window.clearTimeout(timeout));

    return () => {
      active = false;
      window.clearTimeout(timeout);
    };
  }, []);

  return (
    <main>
      <h1>Zapbeam</h1>
      <p>Local-first media optimization.</p>
      <p role="status">
        {status === "connecting" && "Connecting to the Rust backend…"}
        {status === "ready" && "Rust backend connected."}
        {status === "error" &&
          "Unable to connect to the backend. Restart Zapbeam to try again. If this persists, check the development terminal."}
      </p>
      <small>
        Development shell · Image optimization is not implemented yet.
      </small>
    </main>
  );
}

export default App;
