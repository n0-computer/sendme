import { useState } from "react";
import { Tab } from "@headlessui/react"
import classNames from "classnames";

import Send from './Send'
import Receive from './Receive'
import "./App.css";

enum Mode {
  Send,
  Receive,
}

function App() {
  const [mode, setMode] = useState<Mode>(Mode.Send);
  
  return (
    <div className="max-w-xl max-h-xl mx-auto mt-10">
      <Tab.Group
        onChange={(i) => {
          setMode(i === 0 ? Mode.Send : Mode.Receive)
        }}
      >
        <Tab.List className='flex space-x-1 p-1'>
          <Tab
            className={({ selected }) =>
                classNames(
                  "w-full rounded-lg py-2.5 text-sm font-medium leading-5 text-irohPurple-500",
                  "ring-white/60 ring-offset-2 ring-offset-irohPurple-400 focus:outline-none focus:ring-2",
                  selected
                    ? "bg-white dark:bg-zinc-800 shadow"
                    : "text-irohPurple-100 hover:bg-white/[0.12] hover:text-irohPurple-200",
                )
              }
          >
            Send
          </Tab>
          <Tab
            className={({ selected }) =>
            classNames(
              "w-full rounded-lg py-2.5 text-sm font-medium leading-5 text-irohPurple-500",
              "ring-white/60 ring-offset-2 ring-offset-irohPurple-400 focus:outline-none focus:ring-2",
              selected
                ? "bg-white dark:bg-zinc-800 shadow"
                : "text-irohPurple-100 hover:bg-white/[0.12] hover:text-irohPurple-200",
            )
          }
          >
            Receive
          </Tab>
        </Tab.List>
        <Tab.Panels>
          <Tab.Panel>
            {mode === Mode.Send ? <Send /> : null}
          </Tab.Panel>
          <Tab.Panel>
            {mode === Mode.Receive ? <Receive /> : null}
          </Tab.Panel>
        </Tab.Panels>
      </Tab.Group>
    </div>
  );
}

export default App;
