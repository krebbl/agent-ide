import AppLayout from "./components/layout/AppLayout";
import AuthGate from "./components/AuthGate";
import { useCloseShortcut } from "./hooks/useCloseShortcut";

function App() {
  useCloseShortcut();

  return (
    <AuthGate>
      <AppLayout />
    </AuthGate>
  );
}

export default App;
