import { BookCard } from "@/components/book-card";
import { db } from "@/lib/db";

export default async function Home() {
  const books = await db.book.findMany();

  return (
    <main>
      <h1>Bookshelf</h1>
      {books.map((book) => (
        <BookCard key={book.id} book={book} />
      ))}
    </main>
  );
}
