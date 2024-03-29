import { useState } from 'react';
import {Scanner} from '@yudiel/react-qr-scanner';

export default function Receive() {
  const [ticket, setTicket] = useState<string | null>(null);
  return (
    <div className="max-w-xl max-h-xl mx-auto my-20 p-20">
      <h1 className="text-2xl">Receive</h1>
      <Scanner
        onResult={(result: string) => {
          alert(result);
          setTicket(result);
          console.log(result)
        }}
        onError={(error) => console.log(error?.message)}
      />
      <p>{ticket}</p>
    </div>
  );
}