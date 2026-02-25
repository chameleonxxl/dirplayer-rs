import { useEffect, useState } from "react";
import { player_set_preview_parent, player_set_preview_font_size } from "vm-rust";

interface PreviewCanvasProps {
  fontSize?: number;
}

export default function PreviewCanvas({ fontSize }: PreviewCanvasProps) {
  const [isMounted, setIsMounted] = useState(false);
  const onBitmapPreviewRef = (ref: HTMLDivElement | null) => {
    setIsMounted(!!ref);
  };
  useEffect(() => {
    if (isMounted) {
      player_set_preview_parent("#bitmapPreview");
    }
    return () => {
      player_set_preview_parent("");
    };
  }, [isMounted]);

  useEffect(() => {
    player_set_preview_font_size(fontSize ?? 0);
  }, [fontSize]);

  return <div id="bitmapPreview" ref={onBitmapPreviewRef}></div>;
}
