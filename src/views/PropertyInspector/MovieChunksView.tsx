import { JsBridgeChunk } from "dirplayer-js-api";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useAppSelector } from "../../store/hooks";
import { get_cast_chunk_list, get_movie_top_level_chunks, get_chunk_bytes } from "vm-rust";
import styles from "./styles.module.css";

const MOVIE_FILE_VALUE = "movie";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function downloadBlob(data: Uint8Array, filename: string) {
  const blob = new Blob([data], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

function ChunkTreeNode({
  chunkId,
  chunk,
  childrenMap,
  chunks,
  depth,
  filterText,
  matchingIds,
  onSave,
}: {
  chunkId: number;
  chunk: JsBridgeChunk;
  childrenMap: Record<number, number[]>;
  chunks: Partial<Record<number, JsBridgeChunk>>;
  depth: number;
  filterText: string;
  matchingIds: Set<number> | null;
  onSave: (chunkId: number, fourcc: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const children = childrenMap[chunkId] || [];
  const hasChildren = children.length > 0;

  // When filtering, auto-expand nodes that have matching descendants
  const isAutoExpanded = matchingIds !== null && hasChildren;
  const isExpanded = isAutoExpanded || expanded;

  const visibleChildren = useMemo(() => {
    if (!isExpanded) return [];
    if (matchingIds === null) return children;
    return children.filter((id) => matchingIds.has(id));
  }, [isExpanded, children, matchingIds]);

  const isDirectMatch =
    matchingIds !== null &&
    filterText &&
    (chunk.fourcc.toLowerCase().includes(filterText) ||
      String(chunkId).includes(filterText) ||
      (chunk.memberName && chunk.memberName.toLowerCase().includes(filterText)));

  return (
    <>
      <div
        className={`${styles.chunkNode} ${isDirectMatch ? styles.chunkNodeMatch : ""}`}
        style={{ paddingLeft: depth * 16 + 4 }}
        onClick={() => setExpanded(!expanded)}
      >
        <span className={styles.chunkExpander}>
          {hasChildren ? (isExpanded ? "\u25BC" : "\u25B6") : " "}
        </span>
        <span className={styles.chunkFourcc}>{chunk.fourcc}</span>
        <span className={styles.chunkId}>#{chunkId}</span>
        {chunk.memberName && (
          <span className={styles.chunkMember}>
            [{chunk.memberNumber}: {chunk.memberName}]
          </span>
        )}
        <span className={styles.chunkSize}>{formatSize(chunk.len)}</span>
        <a
          className={styles.chunkSave}
          href="#"
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            onSave(chunkId, chunk.fourcc);
          }}
          title="Save chunk content to file"
        >
          (Save)
        </a>
      </div>
      {isExpanded &&
        visibleChildren.map((childId) => {
          const childChunk = chunks[childId];
          if (!childChunk) return null;
          return (
            <ChunkTreeNode
              key={childId}
              chunkId={childId}
              chunk={childChunk}
              childrenMap={childrenMap}
              chunks={chunks}
              depth={depth + 1}
              filterText={filterText}
              matchingIds={matchingIds}
              onSave={onSave}
            />
          );
        })}
    </>
  );
}

export default function MovieChunksView() {
  const [filterText, setFilterText] = useState("");
  const [selectedSource, setSelectedSource] = useState<string>(MOVIE_FILE_VALUE);
  const [chunks, setChunks] = useState<Partial<Record<number, JsBridgeChunk>>>({});
  const castNames = useAppSelector((state) => state.vm.castNames);
  const isMovieLoaded = useAppSelector((state) => state.vm.isMovieLoaded);

  // Fetch chunks when source changes
  useEffect(() => {
    if (!isMovieLoaded) {
      setChunks({});
      return;
    }

    try {
      if (selectedSource === MOVIE_FILE_VALUE) {
        const result = get_movie_top_level_chunks();
        setChunks(result || {});
      } else {
        const castNumber = Number(selectedSource);
        if (castNumber > 0) {
          const result = get_cast_chunk_list(castNumber);
          setChunks(result || {});
        } else {
          setChunks({});
        }
      }
    } catch (e) {
      console.error("Failed to fetch chunks", e);
      setChunks({});
    }
  }, [selectedSource, isMovieLoaded]);

  // The cast number to use for save (0 = main movie file)
  const saveCastNumber = selectedSource === MOVIE_FILE_VALUE ? 0 : Number(selectedSource);

  const handleSave = useCallback((chunkId: number, fourcc: string) => {
    try {
      const bytes = get_chunk_bytes(saveCastNumber, chunkId);
      if (bytes) {
        downloadBlob(bytes, `${fourcc.trim()}_${chunkId}.bin`);
      } else {
        console.warn("No data found for chunk", chunkId);
      }
    } catch (e) {
      console.error("Failed to save chunk", chunkId, e);
    }
  }, [saveCastNumber]);

  // Build parent-child map and find root chunks
  const { childrenMap, rootIds } = useMemo(() => {
    const childrenMap: Record<number, number[]> = {};
    const rootIds: number[] = [];
    const allIds = Object.keys(chunks).map(Number);

    allIds.forEach((id) => {
      const chunk = chunks[id];
      if (!chunk) return;
      if (chunk.owner != null && chunks[chunk.owner] != null) {
        if (!childrenMap[chunk.owner]) childrenMap[chunk.owner] = [];
        childrenMap[chunk.owner].push(id);
      } else {
        rootIds.push(id);
      }
    });

    // Sort children and roots by ID
    Object.keys(childrenMap).forEach((key) => {
      childrenMap[Number(key)].sort((a, b) => a - b);
    });
    rootIds.sort((a, b) => a - b);

    return { childrenMap, rootIds };
  }, [chunks]);

  // When filtering, compute which IDs match and which ancestors need to be visible
  const matchingIds = useMemo<Set<number> | null>(() => {
    const lower = filterText.toLowerCase().trim();
    if (lower.length === 0) return null;

    const directMatches = new Set<number>();
    Object.entries(chunks).forEach(([idStr, chunk]) => {
      if (!chunk) return;
      const id = Number(idStr);

      if (
        chunk.fourcc.toLowerCase().includes(lower) ||
        String(id).includes(lower) ||
        (chunk.memberName && chunk.memberName.toLowerCase().includes(lower))
      ) {
        directMatches.add(id);
      }
    });

    // Walk up from each match to include all ancestors
    const visible = new Set<number>(Array.from(directMatches));
    Array.from(directMatches).forEach((id) => {
      let current = id;
      while (true) {
        const chunk = chunks[current];
        if (!chunk || chunk.owner == null || chunks[chunk.owner] == null) break;
        if (visible.has(chunk.owner)) break;
        visible.add(chunk.owner);
        current = chunk.owner;
      }
    });

    return visible;
  }, [filterText, chunks]);

  const visibleRoots = useMemo(() => {
    if (matchingIds === null) return rootIds;
    return rootIds.filter((id) => matchingIds.has(id));
  }, [rootIds, matchingIds]);

  const lowerFilter = filterText.toLowerCase().trim();

  return (
    <div className={styles.movieChunksContainer}>
      <div className={styles.chunkToolbar}>
        <select
          className={styles.chunkCastFilter}
          value={selectedSource}
          onChange={(e) => setSelectedSource(e.target.value)}
        >
          <option value={MOVIE_FILE_VALUE}>Movie file</option>
          {castNames.map((name, i) => (
            <option key={i + 1} value={i + 1}>
              {name || `Cast ${i + 1}`}
            </option>
          ))}
        </select>
        <input
          className={styles.chunkSearchInput}
          type="text"
          placeholder="Filter by fourcc, ID, or member name..."
          value={filterText}
          onChange={(e) => setFilterText(e.target.value)}
        />
        {filterText && (
          <button
            className={styles.chunkSearchClear}
            onClick={() => setFilterText("")}
          >
            &times;
          </button>
        )}
      </div>
      <div className={styles.chunkTree}>
        {visibleRoots.length === 0 && (
          <div style={{ padding: 8, color: "#999", fontSize: 12 }}>
            {filterText ? "No chunks match the filter." : "No chunks found."}
          </div>
        )}
        {visibleRoots.map((id) => {
          const chunk = chunks[id];
          if (!chunk) return null;
          return (
            <ChunkTreeNode
              key={id}
              chunkId={id}
              chunk={chunk}
              childrenMap={childrenMap}
              chunks={chunks}
              depth={0}
              filterText={lowerFilter}
              matchingIds={matchingIds}
              onSave={handleSave}
            />
          );
        })}
      </div>
    </div>
  );
}
