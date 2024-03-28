import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from '@tauri-apps/api/event'
import QRCode from "react-qr-code";

import "./App.css";
import FileUpload from "./FileUpload";

function App() {
  const [ticket, setTicket] = useState("");
  const [files, setFiles] = useState<FileList | null>(null)

  useEffect(() => {
    listen("tauri://file-drop", (event) => {
      setFiles(event.payload as FileList)
    });
  }, [listen]);

  async function get_ticket() {
    // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
    const result: string = await invoke("send", { files });
    console.log(result);
    setTicket(result);
  }

  if (ticket === "") {
    return (
      <div className="max-w-xl max-h-xl mx-auto my-20 p-20">
        <FileUpload 
          files={files}
          onChange={(files) => {
            console.log("chose files", files);
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

export default App;
