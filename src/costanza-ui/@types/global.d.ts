// esint-disable import/extensions

type ElmInitialization<T> = {
  node?: HTMLElement | null;
  flags?: T;
};

type ElmSubscriber<D> = {
  subscribe: (handler: (content: D) => void) => void;
};

type ElmPublisher<D> = {
  send: (content: D) => void;
};

type ElmApplication<P> = {
  ports: P;
};

type ElmMain<T, P> = {
  init: (opts: ElmInitialization<T>) => ElmApplication<P>;
};

type ElmRuntime<T, P> = {
  Main: ElmMain<T, P>;
};
