import { Titlebar } from "@/components/Titlebar";
import { DevSession } from "@/views/DevSession";

function App() {
  return (
    <div className="flex h-full flex-col">
      <Titlebar />
      <main className="flex min-h-0 flex-1 flex-col">
        <DevSession />
      </main>
    </div>
  );
}

export default App;
