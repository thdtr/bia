type Book = { id: string; title: string; author: string };

export function BookCard({ book }: { book: Book }) {
  return (
    <article>
      <h2>{book.title}</h2>
      <p>{book.author}</p>
    </article>
  );
}
