import { useEffect, useRef, useState } from 'react';
import styles from './styles.module.css';
import { eval_command } from "vm-rust";
import { useAppSelector } from '../../store/hooks';
import { DebugMessage, selectDebugMessages } from '../../store/vmSlice';

function BitmapCanvas({ width, height, data }: { width: number; height: number; data: Uint8Array }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const imageData = new ImageData(
      new Uint8ClampedArray(data.buffer, data.byteOffset, data.byteLength),
      width,
      height,
    );
    ctx.putImageData(imageData, 0, 0);
  }, [width, height, data]);

  return <canvas ref={canvasRef} className={styles.bitmapCanvas} />;
}

function DebugMessageEntry({ message }: { message: DebugMessage }) {
  switch (message.type) {
    case 'text':
      return <span>{message.content}{'\n'}</span>;
    case 'bitmap':
      return <BitmapCanvas width={message.width} height={message.height} data={message.data} />;
  }
}

export default function MessageInspector() {
  const [command, setCommand] = useState('');
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const debugMessages = useAppSelector(({ vm }) => selectDebugMessages(vm));
  const messageLogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messageLogRef.current?.scrollTo(0, messageLogRef.current.scrollHeight);
  }, [debugMessages]);

  const handleEvaluate = () => {
    try {
      if (command.trim()) {
        eval_command(command);
        setHistory(prev => [...prev, command]);
        setHistoryIndex(-1);
      }
      setCommand('');
    } catch (err) {
      console.error(err);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleEvaluate();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (history.length > 0) {
        const newIndex = historyIndex === -1 ? history.length - 1 : Math.max(0, historyIndex - 1);
        setHistoryIndex(newIndex);
        setCommand(history[newIndex]);
      }
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (historyIndex !== -1) {
        const newIndex = Math.min(history.length - 1, historyIndex + 1);
        if (newIndex === history.length - 1 && historyIndex === history.length - 1) {
          setHistoryIndex(-1);
          setCommand('');
        } else {
          setHistoryIndex(newIndex);
          setCommand(history[newIndex]);
        }
      }
    }
  };

  return (
    <div className={styles.container}>
      <div ref={messageLogRef} className={styles.debugMessages}>
        {debugMessages.map((msg, i) => (
          <DebugMessageEntry key={i} message={msg} />
        ))}
      </div>
      <div className={styles.evalSection}>
        <div className={styles.inputGroup}>
          <input
            type="text"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Enter command to evaluate..."
            className={styles.commandInput}
          />
          <button onClick={handleEvaluate} className={styles.evalButton}>
            Evaluate
          </button>
        </div>
      </div>
    </div>
  )
}
