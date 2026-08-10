export const relativeTime = (timestamp: number) => {
  const days = Math.floor((Date.now() / 1000 - timestamp) / 86400);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  return `${days}d`;
};
