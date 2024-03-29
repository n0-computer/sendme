import { useEffect, useState } from 'react';
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from '@tauri-apps/api/event'
import QRCode from "react-qr-code";

import FileUpload from "./FileUpload";

enum Mode {
  Choosing,
  Sending,
}

export default function Send() {
  const [mode, setMode] = useState<Mode>(Mode.Choosing);
  const [ticket, setTicket] = useState("");

  switch (mode) {
    case Mode.Choosing:
      return <Choose onChosen={(ticket) => {
        setTicket(ticket);
        setMode(Mode.Sending);
      }} />;
    case Mode.Sending:
      return <Share ticket={ticket} />;
  }
}

function Choose({ onChosen }: { onChosen: (ticket: string) => void }) {
  const [files, setFiles] = useState<FileList | null>(null)

  useEffect(() => {
    listen("tauri://file-drop", (event) => {
      setFiles(event.payload as FileList)
    });
  }, [listen]);

  async function get_ticket() {
    const ticket: string = await invoke("send", { files });
    onChosen(ticket);
  }

  return (
    <div className="max-w-xl max-h-xl mx-auto my-20 p-20">
      <FileUpload 
        files={files}
        onChange={(files) => {
          setFiles(files);
        }}
      />
      <button
        className="mt-4 px-4 py-2 bg-blue-500 text-white rounded-lg"
        onClick={get_ticket}>
        Get Ticket
      </button>
    </div>
  );
}

function Share({ ticket }: { ticket: string }) {
  return (
    <div className="max-w-xl max-h-xl mx-auto my-20 p-20">
      <QRCode
        value={ticket}
        size={256}
        style={{
          height: "auto",
          maxWidth: "100%",
          width: "100%",
          margin: "auto",
        }}
      />
    </div>
  );
}