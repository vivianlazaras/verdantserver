for file in $(ls); do
    echo $file
    cat $file | wc -l
done