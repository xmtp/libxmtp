export class AsyncStream {
  constructor() {
    this.queue = [];
    this.resolveNext = null;
    this.done = false;
  }

  callback = (err, value) => {
    if (err) {
      console.error("stream error", err);
      this.stop();
      return;
    }

    if (this.done) {
      return;
    }

    if (this.resolveNext) {
      this.resolveNext({ value, done: false });
      this.resolveNext = null;
    } else {
      this.queue.push(value);
    }
  };

  stop = () => {
    this.done = true;
    if (this.resolveNext) {
      this.resolveNext({ value: undefined, done: true });
    }
  };

  next = () => {
    if (this.queue.length > 0) {
      return { value: this.queue.shift(), done: false };
    } else if (this.done) {
      return { value: undefined, done: true };
    } else {
      return new Promise((resolve) => {
        this.resolveNext = resolve;
      });
    }
  };

  [Symbol.asyncIterator]() {
    return this;
  }
}
