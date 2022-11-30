/**
 * This file contains the code that will take care of initializing the elm runtime and
 * controlling our websocket connection with the server. This involves sending two types
 * of serialized JSON messages through the elm port:
 *
 * 1. `control` messages that are used to communicate to the elm runtime when the websocket
 *    itself is opened and closed.
 * 2. `websocket` messages that are effectively a thing wrapper around an already serialized
 *    JSON message that we received from the server. For now, this is preferred to deserializing
 *    and re-serializing here so we can keep the business logic here minimmal.
 *
 * Similarly, the elm runtime is able to send messages back out to us here through a separate port.
 * That port is also split into a `control`/`websocket` architecture, where the `control` messages
 * are used at this later to control attempts to open the websocket, and `websocket` messages are sent
 * as-is along to the underlying websocket.
 */
type Flags = {
  apiRoot: string;
  uiRoot: string;
  loginURL: string;
  logoutURL: string;
  version: string;
};

type Ports = {
  messageReceiver: ElmPublisher<string>;
  sendMessage: ElmSubscriber<string>;
};

declare const Elm: ElmRuntime<Flags, Ports>;

const enum MessageKinds {
  WEBSOCKET = "websocket",
  CONTROL = "control",
}

type TElmMessage =
  | { kind: MessageKinds.WEBSOCKET; payload: string }
  | { kind: MessageKinds.CONTROL };

(function () {
  function metaValue(metaName: string): string | undefined {
    const container = document.querySelector(`meta[name="${metaName}"]`);
    return container ? container.getAttribute("content") ?? void 0 : void 0;
  }

  function boot(): void {
    const apiRoot = metaValue("apiRoot");
    const wsURL = metaValue("wsURL");
    const version = metaValue("version");
    const loginURL = metaValue("loginURL");
    const logoutURL = metaValue("logoutURL");
    const uiRoot = metaValue("uiRoot");

    if (!apiRoot || !version || !loginURL || !uiRoot || !logoutURL || !wsURL) {
      console.error("unable to create elm runtime environment");

      return void 0;
    }

    console.log("booting");
    const flags = { apiRoot, version, loginURL, uiRoot, logoutURL };
    const app = Elm.Main.init({ flags });

    let websocket: WebSocket | undefined = void 0;
    let messageCount = 0;

    // The callback that will be used when our websocket is successfully opened.
    const open = () => {
      app.ports.messageReceiver.send(
        JSON.stringify({ kind: MessageKinds.CONTROL, connected: true })
      );
    };

    // The callback that will be used when our websocket errors. removes callbacks.
    const cleanup = () => {
      messageCount = 0;
      app.ports.messageReceiver.send(
        JSON.stringify({ kind: MessageKinds.CONTROL, connected: false })
      );

      if (!websocket) {
        return;
      }

      const temp = websocket;
      websocket = void 0;

      console.log("cleaning up websocket");
      try {
        temp.close();
      } catch (error) {
        console.warn("unable to close previous websocket");
      }

      temp.removeEventListener("open", open);
      temp.removeEventListener("close", cleanup);
      temp.removeEventListener("error", cleanup);
    };

    const ondata = (event: MessageEvent) => {
      console.log(`[${messageCount}] message from server`, {
        data: event.data,
      });
      messageCount += 1;

      // Pass along the message as a string value within a json-serialized `websocket` message,
      // where the elm runtime will handle deserializing the outer first, following by the inner
      // `payload` string.
      app.ports.messageReceiver.send(
        JSON.stringify({ kind: "websocket", payload: event.data as string })
      );
    };

    // This function is responsible for connecting to, and adding our websocket event listeners
    // that will send messages into the elm runtime via its `ports`.
    function connect(url: string): void {
      console.log("attempting to create new websocket");
      if (websocket) {
        cleanup();
      }

      try {
        websocket = new WebSocket(url);
      } catch (error) {
        console.warn("Unable to establish websocket connection", error);
        websocket = void 0;
        return;
      }

      if (!websocket) {
        console.warn("Unable to establish websocket connection");
        websocket = void 0;
        return;
      }

      websocket.addEventListener("open", open);
      websocket.addEventListener("close", cleanup);
      websocket.addEventListener("error", cleanup);
      websocket.addEventListener("message", ondata);
    }

    // This function is used to subscribe to messages received from elm. These will either be bound
    // for the websocket to the server, or are intended for us here in js land.
    function processElmMessage(content: string): void {
      console.log("processing message from elm runtime", { content });
      try {
        const parsed: TElmMessage = JSON.parse(content);
        switch (parsed.kind) {
          case MessageKinds.WEBSOCKET:
            websocket?.send(parsed.payload);
            break;

          // todo: provide actual information inside of here that will control this outer, js
          // layer. currently all this is doing is being a way for elm to connect our ws.
          case MessageKinds.CONTROL:
            connect(wsURL || "");
            break;
          default:
            console.warn("Unrecognized elm message", content);
            break;
        }
      } catch (error) {
        console.error("Problem handling elm message", error);
      }
    }

    app.ports.sendMessage.subscribe(processElmMessage);
  }

  window.addEventListener("DOMContentLoaded", boot);
})();
