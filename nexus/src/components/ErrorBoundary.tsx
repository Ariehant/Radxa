import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  /** Changing this key clears a caught error (e.g. when a different file opens). */
  resetKey?: string;
  label?: string;
}

interface State {
  error: Error | null;
  key?: string;
}

/** Contains a crashing view so the rest of the workspace keeps working. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, key: this.props.resetKey };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    return props.resetKey !== state.key ? { error: null, key: props.resetKey } : null;
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.warn(`[${this.props.label ?? "view"}] crashed:`, error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="error-boundary" role="alert">
        <h3>{this.props.label ?? "This view"} hit an error</h3>
        <pre>{this.state.error.message}</pre>
        <p>Your files are safe — nothing was written. You can retry, or open something else.</p>
        <button onClick={() => this.setState({ error: null })}>Retry</button>
      </div>
    );
  }
}
