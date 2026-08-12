import { Avatar } from "@ora/ui";

function App() {
  return (
    <main aria-label="Ora workspace" className="flex h-dvh items-center justify-center bg-primary">
      <h1 className="sr-only">Ora</h1>
      <Avatar initials="EW" size="lg" />
    </main>
  );
}

export default App;
