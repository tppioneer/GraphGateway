import Overview from "./pages/Overview";
import "./App.css";

function App() {
  return (
    <div className="app-shell">
      <header className="app-header">
        <h1>GraphGateway Desktop</h1>
      </header>
      <main className="app-main">
        <Overview />
      </main>
    </div>
  );
}

export default App;
