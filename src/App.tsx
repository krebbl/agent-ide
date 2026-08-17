import AppLayout from "./components/layout/AppLayout";
import AuthGate from "./components/AuthGate";

function App() {
  return (
    <AuthGate>
      <AppLayout />
    </AuthGate>
  );
}

export default App;
